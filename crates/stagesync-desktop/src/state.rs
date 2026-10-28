//! Shared data types for the StageSync desktop application.
//!
//! Everything in this module is plain data or a pure function over that data, so it can
//! be unit tested without a window.

use std::path::{Path, PathBuf};

use stagesync_core::audio::{SourceMeta, WavFormat};
use stagesync_core::matching::AlignmentReport;
use stagesync_core::render::Layout;

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
    let model = alignment.calibrated_model.or(alignment.direct_model);
    TargetMetrics {
        offset_secs: model.map(|m| m.offset),
        drift_ppm: model.map(|m| m.drift_ppm()),
        accumulated_drift_ms: model
            .map(|m| m.accumulated_drift_ms(0.0, alignment.source.duration_secs)),
        coverage: Some(alignment.coverage.fraction),
    }
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
    }
}

/// A short label for an output sample format.
pub fn format_label(format: WavFormat) -> &'static str {
    match format {
        WavFormat::Float32 => "32-bit float",
        WavFormat::Int16 => "16-bit PCM",
    }
}

/// A file name without extension, safe to use as an output name.
pub fn stem(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let cleaned: String = base
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
}
