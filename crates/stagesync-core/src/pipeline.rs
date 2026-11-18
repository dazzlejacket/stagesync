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
use crate::matching::drift::{
    assess, fit_offset_only, fit_with_uncertainty, DriftMethod, DriftOptions, DriftStatus,
    MethodFigures,
};
use crate::matching::fine::{fine_match, spectral_offsets, FineOptions, FineResult};
use crate::matching::model::LinearModel;
use crate::matching::segments::{
    analyse_source, merge_timeline, SegmentOptions, SegmentationInput,
};
use crate::matching::waveform::{
    extract_reference_windows, extract_source_windows, measure_from_windows, WaveformDrift,
    WaveformOptions,
};
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
    /// The waveform-domain drift estimator.
    pub waveform: WaveformOptions,
    /// The thresholds that decide whether a drift may be applied.
    pub drift: DriftOptions,
    /// Whether to compute the log-mel cross-check at all.
    pub compute_log_mel: bool,
    /// Whether to measure the drift on the raw waveform as an independent second opinion.
    ///
    /// This costs one extra decode of each file. It is worth it: it is the only way to
    /// tell a drift that was *measured* from one that was *guessed*.
    pub measure_waveform_drift: bool,
    /// Force an offset-only correction, whatever the evidence says.
    ///
    /// The user asked for it, so it is applied — but the report still records what the
    /// evidence was, so the choice is visible rather than hidden.
    pub force_offset_only: bool,
    /// Frames per decoded block.
    pub block_frames: usize,
    /// An optional manual anchor, used when the automatic lock fails.
    ///
    /// This anchor applies to every source that has no source-specific anchor.
    pub manual_anchor: Option<ManualAnchor>,
    /// Manual anchors for individual sources, keyed by the recording's identity.
    ///
    /// A recording set usually contains several files, and a single global anchor would
    /// drag every one of them onto the same correspondence. These anchors are honoured
    /// per recording and take precedence over [`AnalysisConfig::manual_anchor`].
    ///
    /// The key identifies the recording by its **content hash** or by its **full
    /// path**, never by its file name: two recordings called `song.wav` in different
    /// folders are two different recordings, and an anchor meant for one of them
    /// must not be applied to the other.
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
            waveform: WaveformOptions::default(),
            drift: DriftOptions::default(),
            compute_log_mel: true,
            measure_waveform_drift: true,
            force_offset_only: false,
            block_frames: 1 << 16,
            manual_anchor: None,
            manual_anchors: Vec::new(),
            cancel: None,
        }
    }
}

impl AnalysisConfig {
    /// The manual anchor that applies to a source.
    ///
    /// The anchor is looked up by the recording's identity: its content hash, or
    /// its full path. A file name is not an identity — two recordings in
    /// different folders share one — so an anchor keyed by a bare name is not
    /// honoured, and the source is reported as unanchored rather than being
    /// moved by somebody else's anchor.
    pub fn anchor_for(&self, source: &crate::audio::SourceMeta) -> Option<ManualAnchor> {
        self.manual_anchors
            .iter()
            .find(|(key, _)| key_identifies(key, source))
            .map(|(_, anchor)| *anchor)
            .or(self.manual_anchor)
    }

    /// `true` when the analysis has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.is_cancelled())
    }
}

/// Whether an anchor key identifies this recording.
///
/// The key may be the recording's content hash, which identifies it exactly, or
/// its full path, which identifies the file. A bare file name identifies
/// neither.
fn key_identifies(key: &str, source: &crate::audio::SourceMeta) -> bool {
    if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
        return key.eq_ignore_ascii_case(&source.sha256);
    }
    let key_path = std::path::Path::new(key);
    if !key_path.is_absolute() {
        return false;
    }
    if key_path == source.path {
        return true;
    }
    // The same file reached by a different spelling of its path — a symbolic
    // link, or a path with a different case — is still the same recording.
    match (
        std::fs::canonicalize(key_path),
        std::fs::canonicalize(&source.path),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
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
    /// Waveform-domain drift measurement and the drift verdict.
    Drift,
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
            Stage::Drift => "Measuring drift",
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

    // One tick for the reference ingest, one for the drift measurement and five for
    // each source: ingest, coarse lock, fine alignment, calibration and segmentation.
    let total_steps = 2 + source_paths.len() * 5;
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

    // --- Sources: matching ----------------------------------------------------
    //
    // The first pass finds *where* each source sits on the reference timeline. The
    // drift verdict then needs a second, independent measurement on the raw
    // waveform, and that measurement wants the reference decoded only once no matter
    // how many sources there are. So the sources are matched first and the waveform
    // measurement is a separate pass.
    let mut work: Vec<SourceWork> = Vec::with_capacity(source_paths.len());

    for source_path in source_paths.iter() {
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
        let anchor = config.anchor_for(&source_meta);

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
                work.push(SourceWork {
                    path: source_path.clone(),
                    meta: source_meta,
                    lock: Some(lock),
                    warnings,
                    matched: false,
                    points: Vec::new(),
                    fine: FineResult::default(),
                    direct_model: LinearModel::new(1.0, 0.0),
                    calibrated: LinearModel::new(1.0, 0.0),
                    trusted: Vec::new(),
                    anchor: None,
                    automatic_offset: 0.0,
                    envelope: source_env,
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
        if let Some(reason) = &fine.fit_rejection {
            warnings.push(format!(
                "{}: the fine measurements did not corroborate each other — {reason}. \
                 The recording's placement rests on the coarse lock alone.",
                source_meta.file_name
            ));
        }

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

        // A manual anchor is an override, not a hint: the model must pass through it.
        // The offset the automatic model had is kept, so that the report can say
        // how far the user's own judgement moved the placement.
        let mut calibrated = calibration.calibrated_model;
        let automatic_offset = calibrated.offset;
        if let Some(anchor) = anchor {
            calibrated.offset = anchor.reference_time - calibrated.scale * anchor.source_time;
        }

        // The trusted points are the ones the envelope fit actually rests on.
        let trusted: Vec<(f64, f64)> = fine
            .points
            .iter()
            .filter(|p| p.confidence.is_trustworthy())
            .map(|p| (p.source_time, p.reference_time))
            .collect();

        // Require credible position evidence before exposing an exportable model.
        // Without a manual anchor:
        // - Clips too short for fine probes (< 8 s) cannot be corroborated automatically.
        // - Clips shorter than the default coarse window (< 60 s) require either a
        //   corroborated fine fit (`fine.model.is_some()` with >= 3 trusted probes) or,
        //   when a single-point outlier caused `fit_rejection`, a strict majority
        //   (>= 2 and > half) of trusted probes supporting `seed_model`.
        // - Longer recordings require at least one trustworthy fine probe (never 0%
        //   coverage) and, if the fine fit was rejected, at least one trustworthy probe
        //   supporting the coarse lock.
        let fine_ran = source_env.duration_secs >= 8.0;
        let short_recording = source_env.duration_secs < config.coarse.window_secs;
        let tol = config.fine.support_tolerance_ms / 1000.0;
        let (_, seed_support) =
            crate::matching::fine::corroboration_support(&trusted, &seed_model, tol);
        let all_fine_pts: Vec<(f64, f64)> = fine
            .points
            .iter()
            .map(|p| (p.source_time, p.reference_time))
            .collect();
        let (_, fine_seed_support) =
            crate::matching::fine::corroboration_support(&all_fine_pts, &seed_model, 0.050);
        let has_credible_evidence = if anchor.is_some() {
            true
        } else if !fine_ran {
            false
        } else if short_recording {
            if fine.model.is_some() && trusted.len() >= 3 {
                true
            } else if fine.fit_rejection.is_some()
                && seed_support >= 2
                && seed_support * 2 > trusted.len()
            {
                true
            } else {
                // Very short clips (e.g. 12 s) can suffer LowPeakSeparation in the
                // narrow 4 s fine window; accept only when the whole-file shape lock
                // is very strong (>= 0.90) AND a strict majority of >= 3 fine probes
                // land within 50 ms of the coarse offset.
                lock.full_range_ncc >= 0.90
                    && fine.points.len() >= 3
                    && fine_seed_support >= 2
                    && fine_seed_support * 2 > fine.points.len()
            }
        } else if trusted.is_empty() {
            false
        } else if fine.fit_rejection.is_some() {
            seed_support >= 1
        } else {
            true
        };

        if !has_credible_evidence {
            warnings.push(
                "no credible position evidence was found for this source; supply an explicit \
                 manual anchor to place it"
                    .into(),
            );
            work.push(SourceWork {
                path: source_path.clone(),
                meta: source_meta,
                lock: Some(lock),
                warnings,
                matched: false,
                points,
                fine,
                direct_model: LinearModel::new(1.0, 0.0),
                calibrated: LinearModel::new(1.0, 0.0),
                trusted: Vec::new(),
                anchor: None,
                automatic_offset: 0.0,
                envelope: source_env,
            });
            continue;
        }

        work.push(SourceWork {
            path: source_path.clone(),
            meta: source_meta,
            lock: Some(lock),
            warnings,
            matched: true,
            points,
            fine,
            direct_model,
            calibrated,
            trusted,
            anchor,
            automatic_offset,
            envelope: source_env,
        });
    }

    // --- Drift measurement ----------------------------------------------------
    //
    // One pass over the reference serves every source. The probes sit at positions
    // that were *not* used to fit the envelope model, so the waveform measurement is
    // an independent opinion about the drift rather than a restatement of the model.
    let mut measurements: Vec<Option<WaveformDrift>> = vec![None; work.len()];
    if config.measure_waveform_drift {
        checkpoint()?;
        tick(
            progress,
            Stage::Drift,
            "measuring the drift on the raw waveform".to_string(),
        );
        let probe_sets: Vec<Vec<(f64, f64)>> = work
            .iter()
            .map(|w| {
                if !w.matched {
                    return Vec::new();
                }
                holdout_probe_positions(&w.trusted, &w.calibrated, config.waveform.max_probes)
            })
            .collect();

        let reference_windows =
            extract_reference_windows(reference_path, &probe_sets, &config.waveform, cancel_ref)?;

        for (index, item) in work.iter_mut().enumerate() {
            if !item.matched || probe_sets[index].is_empty() {
                continue;
            }
            checkpoint()?;
            let source_windows = extract_source_windows(
                &item.path,
                &probe_sets[index],
                &config.waveform,
                cancel_ref,
            )?;
            let empty = Vec::new();
            let windows = reference_windows.get(index).unwrap_or(&empty);
            measurements[index] = Some(measure_from_windows(
                &probe_sets[index],
                &source_windows,
                windows,
                &config.waveform,
            )?);
        }
    }

    // --- Verdict, residuals and segmentation ----------------------------------
    let mut alignments: Vec<SourceAlignment> = Vec::with_capacity(work.len());

    for (index, item) in work.into_iter().enumerate() {
        let mut warnings = item.warnings;
        let mut points = item.points;

        if !item.matched {
            alignments.push(SourceAlignment {
                source: item.meta,
                map: crate::matching::model::TimeMap::from_segments(vec![]),
                coarse: item.lock,
                fine_measured: 0,
                fine_skipped: 0,
                fine_skip_reasons: Default::default(),
                direct_model: None,
                calibrated_model: None,
                drift: None,
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
                override_state: None,
                warnings,
            });
            continue;
        }

        // The envelope's own measurement of the drift, and how sure it is.
        let direct_fit = fit_with_uncertainty(&item.trusted);
        let envelope_figures = envelope_method_figures(&item.calibrated, direct_fit, &item.trusted);

        let mut waveform_figures: Option<MethodFigures> = None;
        let mut waveform_points: Vec<MatchPoint> = Vec::new();
        match &measurements[index] {
            Some(measurement) => {
                waveform_figures = waveform_method_figures(measurement);
                waveform_points = measurement
                    .probes
                    .iter()
                    .map(|p| MatchPoint {
                        source_time: p.source_time,
                        reference_time: p.reference_time,
                        lag: p.reference_time - p.source_time,
                        confidence: Confidence::new(
                            if p.counted { 1.0 } else { 0.0 },
                            if p.counted {
                                UncertaintyReason::None
                            } else {
                                UncertaintyReason::LowPeakSeparation
                            },
                        ),
                        method: MatchMethod::WaveformPhat,
                        residual_ms: 0.0,
                        note: p.note.clone().unwrap_or_default(),
                    })
                    .collect();
                for note in &measurement.notes {
                    warnings.push(note.clone());
                }
            }
            None if config.measure_waveform_drift => warnings.push(
                "there were no positions left over to probe on the raw waveform, so the drift rests \
                 on the envelope measurement alone"
                    .to_string(),
            ),
            None => {}
        }

        let mut assessment = assess(
            envelope_figures,
            waveform_figures,
            config.measure_waveform_drift,
            item.calibrated,
            &config.drift,
        );

        if config.force_offset_only {
            let offset =
                fit_offset_only(&item.trusted).map_or(item.calibrated.offset, |m| m.offset);
            // The evidence is still reported, but the notes must not describe a
            // decision the user overrode.
            let mut notes = vec![
                "offset-only synchronisation was requested, so the drift was not applied and the \
                 rate was left alone"
                    .to_string(),
            ];
            notes.extend(assessment.notes.drain(..).filter(|note| {
                !note.contains("was used because")
                    && !note.contains("the envelope estimate was used")
                    && !note.contains("the waveform estimate was used")
            }));
            assessment.notes = notes;
            assessment.status = DriftStatus::OffsetOnly;
            assessment.method = DriftMethod::OffsetOnly;
            assessment.model = LinearModel::new(1.0, offset);
            assessment.drift_ppm = 0.0;
            assessment.slope_se_ppm = 0.0;
        }

        // The manual anchor is an override *layer*: it is applied to whichever model the
        // drift assessment settled on. Before this, the anchor was applied to the
        // envelope model only, and a waveform measurement — which is usually the
        // better one — quietly discarded it, so the placement the user asked for
        // was not the placement that was used.
        let mut override_state = None;
        if let Some(anchor) = item.anchor {
            // The automatic offset is the one the model would have had without the
            // override: the envelope's own when the envelope model was kept, and
            // the waveform's when the waveform model was kept.
            let automatic_offset = match assessment.method {
                DriftMethod::Envelope => item.automatic_offset,
                DriftMethod::Waveform | DriftMethod::OffsetOnly => assessment.model.offset,
            };
            let applied_offset =
                anchor.reference_time - assessment.model.scale * anchor.source_time;
            let shift_ms = (applied_offset - automatic_offset) * 1000.0;
            assessment.model = LinearModel::new(assessment.model.scale, applied_offset);
            warnings.push(format!(
                "a manual anchor was applied: the placement was moved {shift_ms:+.1} ms relative to \
                 the automatic match"
            ));
            override_state = Some(crate::matching::OverrideState {
                anchor_source_time: anchor.source_time,
                anchor_reference_time: anchor.reference_time,
                automatic_offset,
                applied_offset,
                shift_ms,
            });
        }

        let final_model = assessment.model;
        let map =
            crate::matching::model::TimeMap::from_linear(final_model, item.meta.duration_secs);

        points.extend(waveform_points);

        // Recompute residuals against the final model. The sign convention is
        // "measured minus model": a positive residual means the measurement sits later
        // in the reference than the model predicts.
        for p in points.iter_mut() {
            p.residual_ms = (p.reference_time - final_model.reference_time(p.source_time)) * 1000.0;
        }

        // --- Segmentation ------------------------------------------------------
        tick(
            progress,
            Stage::Segmentation,
            format!("analysing coverage of {}", item.meta.file_name),
        );
        // The segmentation works from the points that actually fitted the model. The
        // spectral points sit at almost the same source times; letting them into the
        // segmentation would split every covered span into slivers.
        let (segments, coverage) = analyse_source(
            &SegmentationInput {
                source_index: index,
                map: &map,
                points: &item.fine.points,
                source_env: &item.envelope,
                reference_env: &reference_env,
                source_duration: item.meta.duration_secs,
                reference_duration,
            },
            &config.segments,
        );

        alignments.push(SourceAlignment {
            source: item.meta,
            map,
            coarse: item.lock,
            fine_measured: item.fine.measured,
            fine_skipped: item.fine.skipped,
            fine_skip_reasons: item.fine.skip_reasons,
            direct_model: Some(item.direct_model),
            calibrated_model: Some(item.calibrated),
            drift: Some(assessment),
            points,
            segments,
            coverage,
            holdout: None,
            override_state,
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

/// The state of one source between the matching stages and the drift verdict.
struct SourceWork {
    /// The source file.
    path: PathBuf,
    /// The file's metadata.
    meta: crate::audio::SourceMeta,
    /// The coarse global lock.
    lock: Option<CoarseLock>,
    /// Warnings raised so far.
    warnings: Vec<String>,
    /// `false` when no correspondence could be established at all.
    matched: bool,
    /// The envelope and spectral match points.
    points: Vec<MatchPoint>,
    /// The fine alignment result.
    fine: FineResult,
    /// The model fitted directly from the fine matches.
    direct_model: LinearModel,
    /// The model after the end-to-end calibration loop.
    calibrated: LinearModel,
    /// The trusted points the envelope fit rests on.
    trusted: Vec<(f64, f64)>,
    /// The manual anchor that applies to this source, when there is one.
    anchor: Option<ManualAnchor>,
    /// The offset the automatic model had, before any anchor was applied.
    ///
    /// It is kept so that the report can say how far the user's own judgement
    /// moved the placement away from the match.
    automatic_offset: f64,
    /// The source's loudness envelope, needed for the segmentation.
    envelope: Envelope,
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

/// Choose positions that were **not** used to fit the model, and where to search for them.
///
/// The fine probes are the points the envelope model was fitted through, so probing
/// there would only re-measure the model. The midpoints between consecutive probes
/// are not: the waveform measurement there is an independent opinion about the drift.
///
/// The reference position each probe searches around is interpolated from the
/// *measured* neighbouring probes rather than read off the model, so a model with the
/// wrong slope cannot drag the search onto the wrong part of the recording.
fn holdout_probe_positions(
    trusted: &[(f64, f64)],
    model: &LinearModel,
    max_probes: usize,
) -> Vec<(f64, f64)> {
    if trusted.len() < 2 || max_probes == 0 {
        return Vec::new();
    }
    let mut sorted: Vec<(f64, f64)> = trusted.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut candidates: Vec<(f64, f64)> = Vec::new();
    for window in sorted.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if x1 <= x0 {
            continue;
        }
        let midpoint = 0.5 * (x0 + x1);
        let centre = 0.5 * (y0 + y1);
        candidates.push((midpoint, centre));
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    // Spread the probes evenly over the recording: the precision of a slope depends on
    // the *span* the points cover, not on how many there are.
    if candidates.len() > max_probes {
        let mut thinned = Vec::with_capacity(max_probes);
        for i in 0..max_probes {
            let index = (i * candidates.len()) / max_probes;
            thinned.push(candidates[index.min(candidates.len() - 1)]);
        }
        candidates = thinned;
    }

    // The interpolated centre is the starting point; the model's prediction is only a
    // fallback for a position that no measured point brackets.
    candidates
        .into_iter()
        .map(|(source_time, centre)| {
            let interpolated = interpolate_reference(&sorted, source_time, centre);
            (source_time, interpolated)
        })
        .map(|(source_time, centre)| {
            // A wildly off centre would send the search into empty material.
            let predicted = model.reference_time(source_time);
            let centre = if (centre - predicted).abs() > 1.0 {
                predicted
            } else {
                centre
            };
            (source_time, centre)
        })
        .collect()
}

/// Interpolate a reference position from measured points, falling back when bracketed.
fn interpolate_reference(points: &[(f64, f64)], x: f64, fallback: f64) -> f64 {
    match points.len() {
        0 => fallback,
        1 => {
            let (x0, y0) = points[0];
            y0 + (x - x0)
        }
        _ => {
            let last = points.len() - 1;
            if x <= points[0].0 {
                let (x0, y0) = points[0];
                let (x1, y1) = points[1];
                let slope = (y1 - y0) / (x1 - x0).max(f64::EPSILON);
                return y0 + slope * (x - x0);
            }
            if x >= points[last].0 {
                let (x0, y0) = points[last - 1];
                let (x1, y1) = points[last];
                let slope = (y1 - y0) / (x1 - x0).max(f64::EPSILON);
                return y1 + slope * (x - x1);
            }
            for window in points.windows(2) {
                let (x0, y0) = window[0];
                let (x1, y1) = window[1];
                if x >= x0 && x <= x1 {
                    let t = (x - x0) / (x1 - x0).max(f64::EPSILON);
                    return y0 + t * (y1 - y0);
                }
            }
            fallback
        }
    }
}

/// The envelope estimator's figures, with the uncertainty that goes with them.
///
/// The uncertainty of the *fitted* slope is the classical standard error. The
/// calibration is a second envelope-domain estimate of the same slope, so the distance
/// it moved the model is added to the uncertainty in quadrature: the calibration is
/// not known to be better than the fit it started from.
fn envelope_method_figures(
    calibrated: &LinearModel,
    direct_fit: Option<crate::matching::SlopeFit>,
    trusted: &[(f64, f64)],
) -> Option<MethodFigures> {
    let direct = direct_fit?;
    let calibration_shift_ppm = (calibrated.drift_ppm() - direct.drift_ppm()).abs();
    let slope_se_ppm = (direct.slope_se_ppm().powi(2) + calibration_shift_ppm.powi(2)).sqrt();

    let residuals: Vec<f64> = trusted
        .iter()
        .map(|(x, y)| y - calibrated.reference_time(*x))
        .collect();
    let residual_rms_ms = if residuals.is_empty() {
        0.0
    } else {
        (residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64).sqrt() * 1000.0
    };

    Some(MethodFigures {
        method: DriftMethod::Envelope,
        drift_ppm: calibrated.drift_ppm(),
        scale: calibrated.scale,
        slope_se_ppm,
        residual_rms_ms,
        points_used: trusted.len(),
        offset_seconds: calibrated.offset,
        mean_correlation: None,
    })
}

/// The waveform estimator's figures, when it produced a usable fit.
fn waveform_method_figures(
    measurement: &crate::matching::waveform::WaveformDrift,
) -> Option<MethodFigures> {
    let fit = measurement.fit?;
    Some(MethodFigures {
        method: DriftMethod::Waveform,
        drift_ppm: fit.drift_ppm(),
        scale: fit.slope,
        slope_se_ppm: fit.slope_se_ppm(),
        residual_rms_ms: fit.residual_rms * 1000.0,
        points_used: fit.points,
        offset_seconds: fit.intercept,
        mean_correlation: Some(measurement.mean_correlation as f64),
    })
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
