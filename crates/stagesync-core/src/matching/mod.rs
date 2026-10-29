//! Matching: coarse global lock, fine alignment, robust drift model and segmentation.

pub mod coarse;
pub mod drift;
pub mod dsp;
pub mod fine;
pub mod model;
pub mod regression;
pub mod segments;
pub mod waveform;

use serde::{Deserialize, Serialize};

use crate::audio::SourceMeta;

pub use coarse::{CoarseCandidate, CoarseLock, CoarseWindow};
pub use drift::{
    assess, fit_offset_only, fit_with_uncertainty, DriftAssessment, DriftMethod, DriftOptions,
    DriftStatus, MethodFigures, SlopeFit,
};
pub use fine::{FineOptions, FineResult, SkipReasons};
pub use model::{LinearModel, MapSegment, TimeMap};
pub use segments::{Coverage, TimelineSpan};
pub use waveform::{WaveformDrift, WaveformOptions, WaveformProbe};

/// Why a match is not fully trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    /// The match is clean.
    None,
    /// The correlation peak did not stand clearly above the sidelobes (periodic material).
    LowPeakSeparation,
    /// The probe window was quiet or mostly silent.
    LowEnergy,
    /// Neighbouring probes disagreed by more than the expected tolerance.
    WeakAgreement,
    /// Very little material overlapped.
    ShortOverlap,
    /// The point was rejected by the robust fit.
    Outlier,
    /// The point lies in a region where the reference itself has no signal.
    ReferenceSilent,
}

impl UncertaintyReason {
    /// Human-readable explanation, used in reports and the GUI.
    pub fn describe(self) -> &'static str {
        match self {
            UncertaintyReason::None => "clean match",
            UncertaintyReason::LowPeakSeparation => {
                "correlation peak not clearly separated from sidelobes (repeated or periodic material)"
            }
            UncertaintyReason::LowEnergy => "probe window is quiet or mostly silent",
            UncertaintyReason::WeakAgreement => "neighbouring probes disagree",
            UncertaintyReason::ShortOverlap => "too little overlapping material",
            UncertaintyReason::Outlier => "rejected as an outlier by the robust fit",
            UncertaintyReason::ReferenceSilent => "reference is silent at this position",
        }
    }
}

/// Confidence attached to a match point or a segment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Confidence {
    /// Fused score in `[0, 1]`.
    pub score: f32,
    /// The dominant reason the score is not 1.
    pub reason: UncertaintyReason,
}

impl Confidence {
    /// Construct a confidence value.
    pub fn new(score: f32, reason: UncertaintyReason) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reason,
        }
    }

    /// A fully trusted match.
    pub fn high() -> Self {
        Self::new(1.0, UncertaintyReason::None)
    }

    /// Band label used by the GUI and the CSV export.
    pub fn band(&self) -> &'static str {
        if self.score >= 0.8 {
            "high"
        } else if self.score >= 0.5 {
            "medium"
        } else {
            "low"
        }
    }

    /// `true` when the match is good enough to drive a correction.
    pub fn is_trustworthy(&self) -> bool {
        self.score >= 0.5
    }
}

/// Which stage produced a match point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMethod {
    /// Coarse, hour-scale envelope correlation.
    CoarseEnvelope,
    /// Fine envelope correlation around the model prediction.
    FineEnvelope,
    /// Fine spectral (log-mel) correlation.
    FineSpectral,
    /// Waveform GCC-PHAT spot measurement.
    WaveformPhat,
    /// Holdout verification point (never used to fit the model).
    Holdout,
}

impl MatchMethod {
    /// Short name for CSV output.
    pub fn name(self) -> &'static str {
        match self {
            MatchMethod::CoarseEnvelope => "coarse_env",
            MatchMethod::FineEnvelope => "fine_env",
            MatchMethod::FineSpectral => "fine_mel",
            MatchMethod::WaveformPhat => "gcc_phat",
            MatchMethod::Holdout => "holdout",
        }
    }
}

/// A single measured correspondence between a source time and a reference time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPoint {
    /// Centre of the probe window, in source seconds.
    pub source_time: f64,
    /// Matched position in the reference, in seconds.
    pub reference_time: f64,
    /// Measured lag (`reference_time - source_time`) in seconds.
    pub lag: f64,
    /// Residual against the fitted model, in milliseconds.
    pub residual_ms: f64,
    /// Confidence of this point.
    pub confidence: Confidence,
    /// Which stage produced it.
    pub method: MatchMethod,
    /// A plain-language note about this point, when there is something to say.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// The kind of a timeline segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// Material present in both the reference and the source.
    Matched,
    /// Reference material with no source coverage at all (recorder off, or never started).
    UncoveredReference,
    /// Source material that lies beyond the end of the reference.
    BeyondReference,
    /// Covered, but the match there is not trustworthy.
    LowConfidence,
    /// A gap inside the source recording (recorder stop).
    RecorderGap,
}

impl SegmentKind {
    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            SegmentKind::Matched => "matched",
            SegmentKind::UncoveredReference => "uncovered reference span",
            SegmentKind::BeyondReference => "beyond reference",
            SegmentKind::LowConfidence => "low confidence",
            SegmentKind::RecorderGap => "recorder gap",
        }
    }
}

/// A continuous stretch of the timeline with one status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// What this stretch represents.
    pub kind: SegmentKind,
    /// Index of the source this segment belongs to (`None` for pure reference gaps).
    pub source_index: Option<usize>,
    /// Start in source seconds (`None` when the segment is not source-backed).
    pub source_start: Option<f64>,
    /// End in source seconds.
    pub source_end: Option<f64>,
    /// Start in reference seconds.
    pub reference_start: f64,
    /// End in reference seconds.
    pub reference_end: f64,
    /// Confidence across the segment.
    pub confidence: Confidence,
    /// Plain-language note for the GUI and the report.
    pub note: String,
}

impl Segment {
    /// Duration of the segment in seconds.
    pub fn duration(&self) -> f64 {
        (self.reference_end - self.reference_start).max(0.0)
    }
}

/// The alignment result for one source against the reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAlignment {
    /// The source file.
    pub source: SourceMeta,
    /// The final time map (after calibration).
    pub map: TimeMap,
    /// The coarse global lock, including every candidate that was considered.
    pub coarse: Option<CoarseLock>,
    /// How many fine probes produced a measurement.
    pub fine_measured: usize,
    /// How many fine probes were skipped (too quiet, or no peak).
    pub fine_skipped: usize,
    /// Why the fine probes were skipped.
    pub fine_skip_reasons: SkipReasons,
    /// The model fitted directly from the fine matches, before calibration.
    pub direct_model: Option<LinearModel>,
    /// The model after the end-to-end calibration loop.
    pub calibrated_model: Option<LinearModel>,
    /// The model the drift assessment settled on, and why.
    ///
    /// This is the model that is actually applied: it is the calibrated model when the
    /// envelope evidence was good enough, the waveform-domain model when the envelope
    /// was too imprecise to trust, and an offset-only model when the drift could not be
    /// established at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift: Option<DriftAssessment>,
    /// All measured match points.
    pub points: Vec<MatchPoint>,
    /// Segments describing coverage and gaps.
    pub segments: Vec<Segment>,
    /// Coverage of the reference by this source.
    pub coverage: Coverage,
    /// Independent holdout verification of the rendered correction.
    pub holdout: Option<crate::verify::HoldoutResult>,
    /// Warnings raised while analysing this source.
    pub warnings: Vec<String>,
}

impl SourceAlignment {
    /// The model that will be applied to this source.
    ///
    /// The drift assessment has the final say, because it is the only stage that knows
    /// whether the drift was measured to a precision that justifies applying it.
    pub fn model(&self) -> Option<LinearModel> {
        self.drift
            .as_ref()
            .map(|d| d.model)
            .or(self.calibrated_model)
            .or(self.direct_model)
    }
}

/// The complete analysis of a reference against one or more sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentReport {
    /// The reference track.
    pub reference: SourceMeta,
    /// Per-source results, in the order the user supplied them.
    pub sources: Vec<SourceAlignment>,
    /// The merged reference timeline, including genuine uncovered spans.
    pub timeline: Vec<TimelineSpan>,
    /// Total duration of the reference timeline, in seconds.
    pub timeline_duration: f64,
}

impl AlignmentReport {
    /// Find the alignment for a given source index.
    pub fn source(&self, index: usize) -> Option<&SourceAlignment> {
        self.sources.get(index)
    }
}
