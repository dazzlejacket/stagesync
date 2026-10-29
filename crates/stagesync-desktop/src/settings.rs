//! The choices the user made, kept between runs.
//!
//! The settings are written as JSON in the platform's configuration folder. They are
//! deliberately small and boring: a folder, a layout, a sample rate, a format, three
//! switches, the files that were open and the size of the window.

use std::path::Path;

use serde::{Deserialize, Serialize};
use stagesync_core::audio::WavFormat;
use stagesync_core::render::Layout;

use crate::platform;

/// The settings, as they are written to disk.
///
/// The names of the layout and the format are stored as text rather than as the
/// core's enumerations, so that a file written by an older build can still be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Where the exports are written.
    pub export_dir: String,
    /// The output layout: `source_aligned`, `timeline_per_source` or `timeline_merged`.
    pub layout: String,
    /// The output sample rate; `None` keeps the rate of each source.
    pub sample_rate: Option<u32>,
    /// The output format: `float32` or `int16`.
    pub format: String,
    /// Whether the log-mel cross-check runs during the analysis.
    pub use_spectral_check: bool,
    /// Whether the drift is measured on the raw waveform as well.
    pub measure_waveform_drift: bool,
    /// Whether the correction is offset-only, whatever the evidence says.
    pub force_offset_only: bool,
    /// The reference recording of the last session.
    pub last_reference: String,
    /// The target recordings of the last session.
    pub last_sources: Vec<String>,
    /// The width of the window.
    pub window_width: f32,
    /// The height of the window.
    pub window_height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            export_dir: String::new(),
            layout: layout_name(Layout::SourceAligned).to_string(),
            sample_rate: None,
            format: format_name(WavFormat::Float32).to_string(),
            use_spectral_check: true,
            measure_waveform_drift: true,
            force_offset_only: false,
            last_reference: String::new(),
            last_sources: Vec::new(),
            window_width: 1400.0,
            window_height: 900.0,
        }
    }
}

impl AppSettings {
    /// Read the settings from the user's configuration folder.
    pub fn load() -> Self {
        Self::load_from(&platform::settings_file())
    }

    /// Read the settings from a given file.
    ///
    /// A file that is missing, unreadable or damaged gives the defaults. The
    /// application has to start even when the settings cannot be read: losing a
    /// preference is an inconvenience, refusing to start is a failure.
    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Write the settings to the user's configuration folder.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&platform::settings_file())
    }

    /// Write the settings to a given file.
    ///
    /// The file is written under a temporary name and then renamed over the old one,
    /// so a crash in the middle of writing cannot leave a half-written file behind.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }
}

/// The name of an output layout, as it is written to the settings file.
pub fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::SourceAligned => "source_aligned",
        Layout::ReferenceTimelinePerSource => "timeline_per_source",
        Layout::ReferenceTimelineMerged => "timeline_merged",
    }
}

/// The layout with that name, or the default when the name is not known.
pub fn layout_from_name(name: &str) -> Layout {
    match name {
        "timeline_per_source" => Layout::ReferenceTimelinePerSource,
        "timeline_merged" => Layout::ReferenceTimelineMerged,
        _ => Layout::SourceAligned,
    }
}

/// The name of an output format, as it is written to the settings file.
pub fn format_name(format: WavFormat) -> &'static str {
    match format {
        WavFormat::Float32 => "float32",
        WavFormat::Int16 => "int16",
    }
}

/// The format with that name, or the default when the name is not known.
pub fn format_from_name(name: &str) -> WavFormat {
    match name {
        "int16" => WavFormat::Int16,
        _ => WavFormat::Float32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_round_trip() {
        for layout in [
            Layout::SourceAligned,
            Layout::ReferenceTimelinePerSource,
            Layout::ReferenceTimelineMerged,
        ] {
            assert_eq!(layout_from_name(layout_name(layout)), layout);
        }
    }

    #[test]
    fn the_format_names_round_trip() {
        for format in [WavFormat::Float32, WavFormat::Int16] {
            assert_eq!(format_from_name(format_name(format)), format);
        }
    }

    #[test]
    fn an_unknown_name_falls_back_to_the_default() {
        assert_eq!(layout_from_name("nonsense"), Layout::SourceAligned);
        assert_eq!(format_from_name("nonsense"), WavFormat::Float32);
    }
}
