//! Shared data types for the StageSync desktop application.
//!
//! Everything in this module is plain data, a pure function over that data, or a
//! short piece of file-system bookkeeping, so it can be unit tested without a window.

use std::path::{Path, PathBuf};

use stagesync_core::audio::{SourceMeta, WavFormat};
use stagesync_core::matching::drift::{DriftAssessment, DriftStatus, MethodFigures};
use stagesync_core::matching::AlignmentReport;
use stagesync_core::render::Layout;

use crate::platform;

/// A media file the user has imported.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Where the file lives on disk.
    pub path: PathBuf,
    /// Metadata, once the background probe has finished.
    pub meta: Option<SourceMeta>,
    /// Why the probe failed, if it did.
    pub error: Option<String>,
}

impl FileEntry {
    /// An entry for a file that has not been probed yet.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            meta: None,
            error: None,
        }
    }

    /// The file name, for display.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// The figures measured for one target, shown in the file list.
#[derive(Debug, Clone, Copy, Default)]
pub struct TargetMetrics {
    /// Reference time of source time zero, in seconds.
    pub offset_secs: Option<f64>,
    /// Drift of the source clock, in parts per million.
    pub drift_ppm: Option<f64>,
    /// The standard error of the drift, in parts per million.
    ///
    /// A drift without it is a number rather than a measurement, so it is
    /// carried alongside the drift wherever the drift is shown.
    pub drift_se_ppm: Option<f64>,
    /// Drift accumulated over the whole source, in milliseconds.
    pub accumulated_drift_ms: Option<f64>,
    /// Fraction of the reference covered by this source.
    pub coverage: Option<f64>,
}

/// Read the measured figures of one target out of a report.
///
/// The report lists its sources in the order they were handed to the pipeline, which is
/// the order of the target list in the file panel.
pub fn metrics_for(report: &AlignmentReport, index: usize) -> TargetMetrics {
    let Some(alignment) = report.sources.get(index) else {
        return TargetMetrics::default();
    };
    let model = alignment.model();
    let (drift_ppm, drift_se_ppm) = match &alignment.drift {
        // Only an established drift carries an uncertainty; an offset-only
        // source has no drift to give an uncertainty for.
        Some(assessment) if assessment.status == DriftStatus::Established => {
            (Some(assessment.drift_ppm), Some(assessment.slope_se_ppm))
        }
        _ => (None, None),
    };
    TargetMetrics {
        offset_secs: model.map(|m| m.offset),
        drift_ppm: drift_ppm.or_else(|| model.map(|m| m.drift_ppm())),
        drift_se_ppm,
        accumulated_drift_ms: model
            .map(|m| m.accumulated_drift_ms(0.0, alignment.source.duration_secs)),
        coverage: Some(alignment.coverage.fraction),
    }
}

/// What has been established about one target.
///
/// The status says what is *known*, not what was measured: a figure that only
/// means something when there is one is not a status, and leading with it hides
/// the difference between "nothing was found" and "nothing was looked for".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetStatus {
    /// The target has not been analysed yet.
    NotAnalysed,
    /// A reliable correspondence was found and nothing needs looking at.
    AutoAligned,
    /// A correspondence was found, but something about it needs looking at.
    NeedsInspection,
    /// No reliable correspondence was found for this target.
    NoReliableMatch,
    /// The user's own anchor decided the placement.
    OverrideApplied,
}

impl TargetStatus {
    /// The text shown for this status.
    pub fn label(self) -> &'static str {
        match self {
            TargetStatus::NotAnalysed => "Not analysed",
            TargetStatus::AutoAligned => "Auto aligned",
            TargetStatus::NeedsInspection => "Needs inspection",
            TargetStatus::NoReliableMatch => "No reliable match",
            TargetStatus::OverrideApplied => "Override applied",
        }
    }

    /// `true` when the target's placement is settled and needs no attention.
    pub fn is_settled(self) -> bool {
        matches!(self, TargetStatus::AutoAligned)
    }
}

/// The status of one target, derived from the report.
pub fn status_for(report: Option<&AlignmentReport>, index: usize) -> TargetStatus {
    let Some(report) = report else {
        return TargetStatus::NotAnalysed;
    };
    let Some(alignment) = report.sources.get(index) else {
        return TargetStatus::NotAnalysed;
    };
    if alignment.model().is_none() {
        return TargetStatus::NoReliableMatch;
    }
    if alignment.override_state.is_some() {
        return TargetStatus::OverrideApplied;
    }
    // Anything the user should look at: a warning the analysis raised, material
    // the reference has that this target does not cover, or a drift that could
    // not be established.
    let drift_unestablished = alignment
        .drift
        .as_ref()
        .map(|assessment| assessment.status != DriftStatus::Established)
        .unwrap_or(true);
    if !alignment.warnings.is_empty() || alignment.coverage.fraction < 0.999 || drift_unestablished
    {
        return TargetStatus::NeedsInspection;
    }
    TargetStatus::AutoAligned
}

/// What has been verified about the exported correction of one target.
///
/// This is deliberately separate from the analysis status: a target can be
/// perfectly aligned and still have an export that was never verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportVerification {
    /// No export has been verified for this target.
    NotVerified,
    /// The export was verified at this many positions.
    Verified {
        /// How many positions were trusted.
        positions: usize,
    },
    /// The export was checked, but there were not enough trusted positions to
    /// say anything about it.
    InsufficientData,
}

impl ExportVerification {
    /// The text shown for this verification state.
    pub fn label(self) -> String {
        match self {
            ExportVerification::NotVerified => "Export not verified".to_string(),
            ExportVerification::Verified { positions } => {
                format!("Export verified at {positions} sampled positions")
            }
            ExportVerification::InsufficientData => {
                "Insufficient data: too few trusted positions to verify the export".to_string()
            }
        }
    }
}

/// The verification state of one target.
pub fn verification_for(report: Option<&AlignmentReport>, index: usize) -> ExportVerification {
    let Some(holdout) = report
        .and_then(|report| report.sources.get(index))
        .and_then(|alignment| alignment.holdout.as_ref())
    else {
        return ExportVerification::NotVerified;
    };
    // The waveform method and the envelope method are two independent opinions.
    // Either of them can carry the statement, so either of them counts.
    let trusted = holdout.counted.max(holdout.envelope_counted);
    if trusted == 0 {
        return ExportVerification::InsufficientData;
    }
    ExportVerification::Verified { positions: trusted }
}

/// A progress update shown in the status bar.
#[derive(Debug, Clone)]
pub struct TaskProgress {
    /// The stage or step in progress.
    pub label: String,
    /// Detail text for the current step.
    pub message: String,
    /// Completion in `[0, 1]`.
    pub fraction: f64,
}

/// What one estimator measured, in the form the window shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSummary {
    /// The drift, with its uncertainty.
    pub drift: String,
    /// The root-mean-square residual of the fit, in milliseconds.
    pub residual_rms_ms: f64,
    /// How many measurements the fit used.
    pub points_used: usize,
    /// The mean correlation of the raw waveform, when the estimator has one.
    pub mean_correlation: Option<f64>,
}

/// The verdict on one source's drift, in the form the window shows it.
///
/// The drift is always shown with the uncertainty that produced it, because a drift
/// without its uncertainty is a number, not a measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftVerdict {
    /// Whether the drift was established, or only the offset was corrected.
    pub status: String,
    /// Which estimator the applied model came from.
    pub method: String,
    /// The drift that will be applied, with its uncertainty.
    pub applied: String,
    /// The envelope estimator's figures.
    pub envelope: Option<MethodSummary>,
    /// The waveform estimator's figures.
    pub waveform: Option<MethodSummary>,
    /// Why the decision went the way it did.
    pub notes: Vec<String>,
}

/// The verdict on a source's drift, ready to be shown.
///
/// `None` when the analysis produced no verdict at all — there was no correspondence
/// to judge, so there is nothing to report.
pub fn drift_verdict(assessment: Option<&DriftAssessment>) -> Option<DriftVerdict> {
    let assessment = assessment?;
    let status = match assessment.status {
        DriftStatus::Established => "established",
        DriftStatus::OffsetOnly => "offset only",
    };
    let applied = match assessment.status {
        DriftStatus::Established => format!(
            "{:+.3} ppm ± {:.3} ppm",
            assessment.drift_ppm, assessment.slope_se_ppm
        ),
        DriftStatus::OffsetOnly => "no drift applied — only the offset was corrected".to_string(),
    };
    Some(DriftVerdict {
        status: status.to_string(),
        method: assessment.method.label().to_string(),
        applied,
        envelope: assessment.envelope.as_ref().map(method_summary),
        waveform: assessment.waveform.as_ref().map(method_summary),
        notes: assessment.notes.clone(),
    })
}

/// The figures of one estimator, in the form the window shows them.
fn method_summary(figures: &MethodFigures) -> MethodSummary {
    MethodSummary {
        drift: format!(
            "{:+.3} ppm ± {:.3} ppm",
            figures.drift_ppm, figures.slope_se_ppm
        ),
        residual_rms_ms: figures.residual_rms_ms,
        points_used: figures.points_used,
        mean_correlation: figures.mean_correlation,
    }
}

/// The recordings inside a set of dropped paths.
///
/// A dropped folder is opened and the audio files inside it are taken; a dropped file
/// is taken as it is. Hidden files and the temporary files that office applications
/// leave behind are skipped, so a folder full of clutter does not fill the list.
pub fn expand_dropped_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            out.extend(audio_files_in(path, 0));
        } else if !platform::is_hidden_file(path) {
            out.push(path.clone());
        }
    }
    out
}

/// The audio files inside a folder, a few levels deep.
fn audio_files_in(dir: &Path, depth: usize) -> Vec<PathBuf> {
    /// How deep the search goes; a concert recording folder is not a file system.
    const MAX_DEPTH: usize = 4;

    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if platform::is_hidden_file(&path) {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => folders.push(path),
            Ok(kind) if kind.is_file() && is_audio_file(&path) => files.push(path),
            _ => {}
        }
    }
    files.sort();
    for folder in folders {
        files.extend(audio_files_in(&folder, depth + 1));
    }
    files
}

/// Whether a file looks like a recording the pipeline can read.
fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "wav" | "flac" | "m4a" | "mp3" | "ogg" | "oga" | "aac" | "opus" | "wma"
            )
        })
        .unwrap_or(false)
}

/// Where and how an export is written.
#[derive(Debug, Clone)]
pub struct ExportSettings {
    /// Destination folder.
    pub dir: String,
    /// Output layout.
    pub layout: Layout,
    /// Output sample rate; `None` keeps the source rate.
    pub sample_rate: Option<u32>,
    /// Output sample format.
    pub format: WavFormat,
    /// The mixer's settings: which recordings go into the mix, and how.
    pub mix: stagesync_core::mixer::MixSettings,
    /// How the mix protects itself against clipping.
    pub protection: stagesync_core::mixer::ClippingProtection,
    /// Whether tracks in the mix are loudness-matched before mixing.
    pub loudness: stagesync_core::mixer::LoudnessMatch,
    /// WAV container format for mixdown exports (`Auto`, `Riff`, `Rf64`, `Bw64`).
    pub container: stagesync_core::mixer::ContainerChoice,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            dir: String::new(),
            layout: Layout::SourceAligned,
            sample_rate: None,
            format: WavFormat::Float32,
            mix: stagesync_core::mixer::MixSettings {
                tracks: Vec::new(),
                master_gain: 1.0,
            },
            protection: stagesync_core::mixer::ClippingProtection::None,
            loudness: stagesync_core::mixer::LoudnessMatch::None,
            container: stagesync_core::mixer::ContainerChoice::Auto,
        }
    }
}

/// The manual anchor the user typed in.
#[derive(Debug, Clone, Copy)]
pub struct AnchorFields {
    /// Position in the source, in seconds.
    pub source_time: f64,
    /// The position in the reference it corresponds to, in seconds.
    pub reference_time: f64,
    /// Whether the anchor is in effect.
    pub active: bool,
}

impl Default for AnchorFields {
    fn default() -> Self {
        Self {
            source_time: 0.0,
            reference_time: 0.0,
            active: false,
        }
    }
}

/// A short label for an output layout.
pub fn layout_label(layout: Layout) -> &'static str {
    match layout {
        Layout::SourceAligned => "Source aligned",
        Layout::ReferenceTimelinePerSource => "Reference timeline, per source",
        Layout::ReferenceTimelineMerged => "Reference timeline, merged",
        Layout::Mixdown => "Mixdown",
        Layout::MixStems => "Synchronised stems",
    }
}

/// A short label for an output sample format.
pub fn format_label(format: WavFormat) -> &'static str {
    match format {
        WavFormat::Float32 => "32-bit float",
        WavFormat::Int16 => "16-bit PCM",
    }
}

/// A short label for a clipping protection.
pub fn protection_label(protection: stagesync_core::mixer::ClippingProtection) -> &'static str {
    use stagesync_core::mixer::ClippingProtection;
    match protection {
        ClippingProtection::None => "None",
        ClippingProtection::SoftClip { .. } => "Soft knee",
        ClippingProtection::AutoGain { .. } => "Automatic gain",
    }
}

/// A short label for a loudness matching mode.
pub fn loudness_label(loudness: stagesync_core::mixer::LoudnessMatch) -> &'static str {
    use stagesync_core::mixer::LoudnessMatch;
    match loudness {
        LoudnessMatch::None => "Keep original levels",
        LoudnessMatch::MatchToLoudest => "Match RMS to loudest",
    }
}

/// A short label for a mix container choice.
pub fn container_label(container: stagesync_core::mixer::ContainerChoice) -> &'static str {
    use stagesync_core::mixer::ContainerChoice;
    match container {
        ContainerChoice::Auto => "Auto (RIFF or RF64)",
        ContainerChoice::Riff => "Standard RIFF/WAV",
        ContainerChoice::Rf64 => "RF64 (>4 GiB)",
        ContainerChoice::Bw64 => "BW64 (Broadcast)",
    }
}

/// A file name without extension, safe to use as an output name.
///
/// The name is made legal on every platform first, so a recording called `CON.wav` or
/// `take 2: live.wav` cannot produce a file the system refuses to write.
pub fn stem(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    if base.trim().is_empty() {
        return "source".to_string();
    }
    let cleaned: String = platform::sanitise_stem(&base)
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    if cleaned.is_empty() {
        "source".to_string()
    } else {
        cleaned
    }
}

/// Format a duration in seconds as `h:mm:ss`.
pub fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--:--".to_string();
    }
    let total = seconds.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{hours}:{minutes:02}:{secs:02}")
}

/// Whether a path looks like a WAV file.
///
/// The hold-out verifier reads the reference with a WAV reader, so it can only be used
/// when the reference really is a WAV.
pub fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagesync_core::matching::drift::DriftMethod;
    use stagesync_core::matching::LinearModel;

    #[test]
    fn stem_drops_the_extension_and_replaces_spaces() {
        assert_eq!(stem("watch recording.wav"), "watch_recording");
        assert_eq!(stem("already_clean.wav"), "already_clean");
        assert_eq!(stem("no_extension"), "no_extension");
        assert_eq!(stem(""), "source");
    }

    #[test]
    fn duration_is_formatted_as_h_m_s() {
        assert_eq!(format_duration(0.0), "0:00:00");
        assert_eq!(format_duration(61.4), "0:01:01");
        assert_eq!(format_duration(3661.0), "1:01:01");
        assert_eq!(format_duration(f64::NAN), "--:--");
        assert_eq!(format_duration(-5.0), "--:--");
    }

    #[test]
    fn wav_detection_is_case_insensitive() {
        assert!(is_wav(Path::new("a/b.wav")));
        assert!(is_wav(Path::new("a/b.WAV")));
        assert!(!is_wav(Path::new("a/b.m4a")));
        assert!(!is_wav(Path::new("a/b")));
    }

    /// The figures of one estimator, for the tests below.
    fn figures(
        method: DriftMethod,
        drift_ppm: f64,
        slope_se_ppm: f64,
        points_used: usize,
        mean_correlation: Option<f64>,
    ) -> MethodFigures {
        MethodFigures {
            method,
            drift_ppm,
            scale: 1.0 + drift_ppm / 1_000_000.0,
            slope_se_ppm,
            residual_rms_ms: 0.5,
            points_used,
            offset_seconds: 0.25,
            mean_correlation,
        }
    }

    #[test]
    fn an_offset_only_verdict_says_that_no_drift_was_applied() {
        let assessment = DriftAssessment {
            status: DriftStatus::OffsetOnly,
            method: DriftMethod::OffsetOnly,
            model: LinearModel::new(1.0, 0.25),
            drift_ppm: 0.0,
            slope_se_ppm: 0.0,
            residual_rms_ms: 0.0,
            points_used: 0,
            envelope: None,
            waveform: None,
            notes: vec!["the drift was not precise enough".to_string()],
        };

        let verdict = drift_verdict(Some(&assessment)).expect("there is a verdict");
        assert_eq!(verdict.status, "offset only");
        assert_eq!(verdict.method, "offset only");
        assert!(
            verdict.applied.contains("no drift applied"),
            "the verdict does not say that no drift was applied: {}",
            verdict.applied
        );
        assert_eq!(verdict.notes.len(), 1, "the notes must not be dropped");
    }

    #[test]
    fn an_established_verdict_shows_both_estimators() {
        let assessment = DriftAssessment {
            status: DriftStatus::Established,
            method: DriftMethod::Waveform,
            model: LinearModel::new(1.000_012_9, 0.25),
            drift_ppm: 12.9,
            slope_se_ppm: 0.1,
            residual_rms_ms: 0.4,
            points_used: 18,
            envelope: Some(figures(DriftMethod::Envelope, 12.5, 0.4, 120, None)),
            waveform: Some(figures(DriftMethod::Waveform, 12.9, 0.1, 18, Some(0.72))),
            notes: vec!["the two estimates agree".to_string()],
        };

        let verdict = drift_verdict(Some(&assessment)).expect("there is a verdict");
        assert_eq!(verdict.status, "established");
        assert_eq!(verdict.method, "waveform");
        assert_eq!(verdict.applied, "+12.900 ppm ± 0.100 ppm");

        let envelope = verdict.envelope.expect("the envelope figures");
        assert_eq!(envelope.points_used, 120);
        assert!(
            envelope.drift.contains("±"),
            "the uncertainty must be shown"
        );

        let waveform = verdict.waveform.expect("the waveform figures");
        assert_eq!(waveform.points_used, 18);
        assert_eq!(waveform.mean_correlation, Some(0.72));
    }

    #[test]
    fn a_source_without_a_verdict_has_nothing_to_report() {
        assert!(drift_verdict(None).is_none());
    }

    #[test]
    fn a_dropped_folder_gives_the_audio_files_inside_it() {
        let dir = tempfile::TempDir::new().expect("the temporary directory");
        let wav = dir.path().join("one.wav");
        let notes = dir.path().join("notes.txt");
        let hidden = dir.path().join(".hidden.wav");
        std::fs::write(&wav, b"not really audio").expect("the file was written");
        std::fs::write(&notes, b"not audio either").expect("the file was written");
        std::fs::write(&hidden, b"hidden").expect("the file was written");

        let dropped = expand_dropped_paths(&[dir.path().to_path_buf()]);
        assert_eq!(dropped, vec![wav]);

        let direct = expand_dropped_paths(std::slice::from_ref(&notes));
        assert_eq!(direct, vec![notes]);

        let skipped = expand_dropped_paths(&[hidden]);
        assert!(skipped.is_empty(), "a hidden file must be skipped");
    }
}
