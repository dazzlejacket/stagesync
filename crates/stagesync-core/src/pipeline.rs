//! The analysis pipeline: files in, an [`AlignmentReport`] out.
//!
//! The pipeline is deliberately staged and observable, because the GUI and the CLI both
//! need to report progress and be able to cancel cleanly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;

use crate::audio::AudioDecoder;
use crate::calibration::{calibrate, CalibrationConfig};
use crate::features::envelope::{extract_envelope, EnvelopeConfig};
use crate::features::mel::{extract_log_mel, LogMel, LogMelConfig};
use crate::features::Envelope;
use crate::matching::coarse::{coarse_lock, CoarseConfig, CoarseLock};
use crate::matching::fine::{fine_match, spectral_offsets, FineOptions};
use crate::matching::model::LinearModel;
use crate::matching::segments::{analyse_source, merge_timeline, SegmentOptions};
use crate::matching::{
    AlignmentReport, Confidence, MatchMethod, MatchPoint, Segment, SegmentKind, SourceAlignment,
    UncertaintyReason,
};

/// A manual anchor supplied by the user when automatic matching fails.
#[derive(Debug, Clone, Copy)]
pub struct ManualAnchor {
    /// Position in the source, in seconds.
    pub source_time: f64,
    /// The position in the reference that this source point corresponds to, in seconds.
    pub reference_time: f64,
}

/// A cooperative cancellation flag.
///
/// The analysis of a multi-hour recording takes minutes, so the user must be able to
/// stop it. The flag is checked at every stage boundary *and* inside the decoding loop,
/// so a cancellation takes effect within a second rather than at the end of a file.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A fresh, not-yet-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// `true` once cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// The underlying flag, for callers that already own an `Arc<AtomicBool>`.
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }

    /// The underlying flag.
    pub fn flag(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }
}

/// The error returned when an analysis is cancelled.
pub const CANCELLED: &str = "the analysis was cancelled";

/// Configuration of the whole analysis.
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Envelope extraction.
    pub envelope: EnvelopeConfig,
    /// Log-mel extraction (used for the independent spectral cross-check).
    pub log_mel: LogMelConfig,
    /// Coarse global lock.
    pub coarse: CoarseConfig,
    /// Fine alignment.
    pub fine: FineOptions,
    /// Segmentation.
    pub segments: SegmentOptions,
    /// End-to-end calibration.
    pub calibration: CalibrationConfig,
    /// Whether to compute the log-mel cross-check at all.
    pub compute_log_mel: bool,
    /// Frames per decoded block.
    pub block_frames: usize,
    /// An optional manual anchor, used when the automatic lock fails.
    ///
    /// This anchor applies to every source that has no source-specific anchor.
    pub manual_anchor: Option<ManualAnchor>,
    /// Manual anchors for individual sources, keyed by the source's file name.
    ///
    /// A recording set usually contains several files, and a single global anchor would
    /// drag every one of them onto the same correspondence. These anchors are honoured
    /// per file and take precedence over [`AnalysisConfig::manual_anchor`].
    pub manual_anchors: Vec<(String, ManualAnchor)>,
    /// A cancellation token. When it is cancelled, [`analyze`] stops at the next
    /// checkpoint and returns an error.
    pub cancel: Option<CancelToken>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            envelope: EnvelopeConfig::default(),
            log_mel: LogMelConfig::default(),
            coarse: CoarseConfig::default(),
            fine: FineOptions::default(),
            segments: SegmentOptions::default(),
            calibration: CalibrationConfig::default(),
            compute_log_mel: true,
            block_frames: 1 << 16,
            manual_anchor: None,
            manual_anchors: Vec::new(),
            cancel: None,
        }
    }
}

impl AnalysisConfig {
    /// The manual anchor that applies to a source with the given file name.
    pub fn anchor_for(&self, file_name: &str) -> Option<ManualAnchor> {
        self.manual_anchors
            .iter()
            .find(|(name, _)| name == file_name)
            .map(|(_, anchor)| *anchor)
            .or(self.manual_anchor)
    }

    /// `true` when the analysis has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.is_cancelled())
    }
}

/// The stage of the pipeline currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Reading and decoding the reference.
    ReferenceIngest,
    /// Reading and decoding a source.
    SourceIngest,
    /// Coarse global lock.
    Coarse,
    /// Fine alignment.
    Fine,
    /// End-to-end calibration.
    Calibration,
    /// Coverage and gap analysis.
    Segmentation,
}

impl Stage {
    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Stage::ReferenceIngest => "Reading reference",
            Stage::SourceIngest => "Reading source",
            Stage::Coarse => "Finding coarse offset",
            Stage::Fine => "Fine alignment",
            Stage::Calibration => "Calibrating",
            Stage::Segmentation => "Analysing coverage",
        }
    }
}

/// A progress notification.
#[derive(Debug, Clone)]
pub struct Progress {
    /// The stage in progress.
    pub stage: Stage,
    /// A human-readable detail message.
    pub message: String,
    /// Overall completion in `[0, 1]`.
    pub fraction: f64,
}

/// A callback for progress notifications.
pub type ProgressSink<'a> = &'a mut dyn FnMut(Progress);

/// Analyse a reference against one or more sources.
pub fn analyze(
    reference_path: &Path,
    source_paths: &[PathBuf],
    config: &AnalysisConfig,
    progress: ProgressSink<'_>,
) -> Result<AlignmentReport> {
    anyhow::ensure!(
        !source_paths.is_empty(),
        "no source recordings were supplied"
    );

    let cancel = config.cancel.as_ref().map(|c| c.flag());
    let cancel_ref = cancel.as_ref().map(|f| f.as_ref());
    let checkpoint = || -> Result<()> {
        if cancel_ref.is_some_and(|f| f.load(Ordering::SeqCst)) {
            anyhow::bail!("{CANCELLED}");
        }
        Ok(())
    };

    let total_steps = 2 + source_paths.len() * 4;
    let mut step = 0usize;
    let mut tick = |progress: &mut dyn FnMut(Progress), stage: Stage, message: String| {
        step += 1;
        progress(Progress {
            stage,
            message,
            fraction: (step as f64 / total_steps as f64).min(1.0),
        });
    };

    // --- Reference -------------------------------------------------------------
    tick(
        progress,
        Stage::ReferenceIngest,
        format!("decoding {}", reference_path.display()),
    );
    checkpoint()?;
    let (reference_meta, reference_env, reference_mel) =
        ingest(reference_path, config, cancel_ref)?;
    let reference_duration = reference_meta.duration_secs;

    // --- Sources ---------------------------------------------------------------
    let mut alignments: Vec<SourceAlignment> = Vec::new();

    for (index, source_path) in source_paths.iter().enumerate() {
        checkpoint()?;
        tick(
            progress,
            Stage::SourceIngest,
            format!("decoding {}", source_path.display()),
        );
        let (source_meta, source_env, source_mel) = ingest(source_path, config, cancel_ref)?;

        // --- Coarse lock -------------------------------------------------------
        checkpoint()?;
        tick(
            progress,
            Stage::Coarse,
            format!("searching for {} in the reference", source_meta.file_name),
        );
        let lock: CoarseLock = coarse_lock(&source_env, &reference_env, &config.coarse);
        let mut warnings: Vec<String> = Vec::new();
        if let Some(reason) = &lock.reason {
            warnings.push(format!("coarse lock: {reason}"));
        }

        // The anchor that applies to *this* source: a source-specific one wins over the
        // global fallback.
        let anchor = config.anchor_for(&source_meta.file_name);

        let seed_model = match (lock.model, anchor) {
            (Some(m), _) => m,
            (None, Some(anchor)) => {
                warnings.push("automatic lock failed; the manual anchor was used instead".into());
                LinearModel::new(1.0, anchor.reference_time - anchor.source_time)
            }
            (None, None) => {
                warnings.push(
                    "no correspondence could be established for this source; it is reported as uncovered"
                        .into(),
                );
                alignments.push(SourceAlignment {
                    source: source_meta,
                    map: crate::matching::model::TimeMap::from_segments(vec![]),
                    coarse: Some(lock),
                    fine_measured: 0,
                    fine_skipped: 0,
                    fine_skip_reasons: Default::default(),
                    direct_model: None,
                    calibrated_model: None,
                    points: Vec::new(),
                    segments: vec![Segment {
                        kind: SegmentKind::UncoveredReference,
                        source_index: Some(index),
                        source_start: None,
                        source_end: None,
                        reference_start: 0.0,
                        reference_end: reference_duration,
                        confidence: Confidence::new(0.0, UncertaintyReason::LowPeakSeparation),
                        note: "no reliable correspondence was found for this source".into(),
                    }],
                    coverage: crate::matching::segments::Coverage {
                        covered_seconds: 0.0,
                        low_confidence_seconds: 0.0,
                        uncovered_seconds: reference_duration,
                        fraction: 0.0,
                    },
                    holdout: None,
                    warnings,
                });
                continue;
            }
        };

        // --- Fine alignment ----------------------------------------------------
        checkpoint()?;
        tick(
            progress,
            Stage::Fine,
            format!("measuring {} against the reference", source_meta.file_name),
        );
        let fine = fine_match(&source_env, &reference_env, &seed_model, &config.fine);
        let direct_model: LinearModel = fine.model.unwrap_or(seed_model);

        // Independent spectral cross-check (a different feature family).
        let mut points: Vec<MatchPoint> = fine.points.clone();
        if config.compute_log_mel {
            if let (Some(src_mel), Some(ref_mel)) = (&source_mel, &reference_mel) {
                let spectral = spectral_offsets(src_mel, ref_mel, &seed_model, &config.fine);
                let comparison = compare_methods(&fine.points, &spectral, 0.050);
                let spectral_count = spectral.len();
                if comparison.disagree > 0 {
                    warnings.push(format!(
                        "the spectral cross-check disagreed with the envelope fit at {} of {} positions \
                         (threshold 50 ms; median disagreement {:.0} ms, largest {:.0} ms); the envelope fit \
                         was kept and the disagreement is reported",
                        comparison.disagree,
                        spectral_count,
                        comparison.median_ms,
                        comparison.max_ms
                    ));
                }
                points.extend(spectral.into_iter().map(|mut p| {
                    p.method = MatchMethod::FineSpectral;
                    p
                }));
            }
        }

        // --- Calibration -------------------------------------------------------
        checkpoint()?;
        tick(
            progress,
            Stage::Calibration,
            format!("calibrating {}", source_meta.file_name),
        );
        let calibration = calibrate(
            &source_env,
            &reference_env,
            &direct_model,
            &config.fine,
            &config.calibration,
        );
        warnings.extend(calibration.warnings.clone());

        // A manual anchor is a hard constraint: the final model must pass through it.
        let mut calibrated = calibration.calibrated_model;
        if let Some(anchor) = anchor {
            calibrated.offset = anchor.reference_time - calibrated.scale * anchor.source_time;
        }

        let map =
            crate::matching::model::TimeMap::from_linear(calibrated, source_meta.duration_secs);

        // Recompute residuals against the final model. The sign convention is
        // "measured minus model": a positive residual means the measurement sits later
        // in the reference than the model predicts.
        for p in points.iter_mut() {
            p.residual_ms = (p.reference_time - calibrated.reference_time(p.source_time)) * 1000.0;
        }

        // --- Segmentation ------------------------------------------------------
        tick(
            progress,
            Stage::Segmentation,
            format!("analysing coverage of {}", source_meta.file_name),
        );
        // The segmentation works from the points that actually fitted the model. The
        // spectral points sit at almost the same source times; letting them into the
        // segmentation would split every covered span into slivers.
        let (segments, coverage) = analyse_source(
            index,
            &map,
            &fine.points,
            &source_env,
            source_meta.duration_secs,
            reference_duration,
            &config.segments,
        );

        alignments.push(SourceAlignment {
            source: source_meta,
            map,
            coarse: Some(lock),
            fine_measured: fine.measured,
            fine_skipped: fine.skipped,
            fine_skip_reasons: fine.skip_reasons,
            direct_model: Some(direct_model),
            calibrated_model: Some(calibrated),
            points,
            segments,
            coverage,
            holdout: None,
            warnings,
        });
    }

    let per_source: Vec<Vec<Segment>> = alignments.iter().map(|a| a.segments.clone()).collect();
    let timeline = merge_timeline(&per_source, reference_duration);

    Ok(AlignmentReport {
        reference: reference_meta,
        sources: alignments,
        timeline,
        timeline_duration: reference_duration,
    })
}

/// Decode a file and extract its features.
fn ingest(
    path: &Path,
    config: &AnalysisConfig,
    cancel: Option<&AtomicBool>,
) -> Result<(crate::audio::SourceMeta, Envelope, Option<LogMel>)> {
    let mut decoder = AudioDecoder::open(path)?;
    let meta = decoder.meta().clone();
    let env = extract_envelope(&mut decoder, config.envelope, config.block_frames, cancel)?;
    decoder.finish();

    let mel = if config.compute_log_mel {
        // The reference only needs the log-mel for the cross-check; computing it for
        // every source as well would double the cost for no extra evidence.
        let mut decoder = AudioDecoder::open(path)?;
        Some(extract_log_mel(
            &mut decoder,
            config.log_mel,
            config.block_frames,
            cancel,
        )?)
    } else {
        None
    };

    Ok((meta, env, mel))
}

/// The outcome of comparing two independent measurement methods.
#[derive(Debug, Clone, Copy, Default)]
pub struct MethodComparison {
    /// How many positions disagreed by more than the threshold.
    pub disagree: usize,
    /// How many positions were compared.
    pub compared: usize,
    /// Median disagreement, in milliseconds.
    pub median_ms: f64,
    /// Largest disagreement, in milliseconds.
    pub max_ms: f64,
}

/// Compare two independent methods and describe how far apart they are.
fn compare_methods(a: &[MatchPoint], b: &[MatchPoint], threshold_secs: f64) -> MethodComparison {
    let mut disagree = 0usize;
    let mut compared = 0usize;
    let mut deltas_ms: Vec<f64> = Vec::new();

    for pa in a {
        if pa.confidence.score < 0.5 {
            continue;
        }
        // Find the nearest point of the other method.
        if let Some(pb) = b.iter().min_by(|x, y| {
            (x.source_time - pa.source_time)
                .abs()
                .partial_cmp(&(y.source_time - pa.source_time).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            if (pb.source_time - pa.source_time).abs() < 1.0 {
                compared += 1;
                let delta_ms = (pb.reference_time - pa.reference_time) * 1000.0;
                deltas_ms.push(delta_ms);
                if delta_ms.abs() > threshold_secs * 1000.0 {
                    disagree += 1;
                }
            }
        }
    }

    let (median_ms, max_ms) = if deltas_ms.is_empty() {
        (0.0, 0.0)
    } else {
        let mut sorted = deltas_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted[sorted.len() / 2];
        let max = sorted.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        (median, max)
    };

    MethodComparison {
        disagree,
        compared,
        median_ms,
        max_ms,
    }
}
