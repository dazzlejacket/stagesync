//! Real multi-track mixing.
//!
//! The mixer plays several synchronised recordings at once. Every recording keeps
//! its own timeline: the drift correction that places it against the reference is
//! the same correction the export applies, computed by the same resampler, so what
//! is heard in a preview is what is written to the file.
//!
//! Four rules govern what the mixer does:
//!
//! * **No channel is ever lost.** A stereo recording keeps its left and right
//!   channels; they are mixed into the output with a matrix, never summed into one
//!   channel and never duplicated. A recording whose channels are in antiphase stays
//!   in antiphase.
//! * **No automatic gain.** A recording that is 37 dB quieter than the reference
//!   stays 37 dB quieter, unless the user asks for a loudness match — and then the
//!   gain that was applied is reported.
//! * **The mix is floating point.** Peaks above 0 dBFS survive the mix and are
//!   written as they are into a 32-bit float file. The protection that keeps a file
//!   inside its format's range is opt-in, and says what it did.
//! * **The reference is not mixed in.** It is the thing the other recordings are
//!   aligned to, not another track. It can be *monitored*, but it is never part of
//!   a mix.
//!
//! The mixer streams: a three-hour concert is mixed a block at a time, and nothing
//! larger than a block of one recording is ever held in memory.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::{AudioDecoder, Container, WavFormat, WavWriter};
use crate::matching::AlignmentReport;
use crate::render::{geometry_timeline, SourceCursor};

pub mod levels;
pub mod pan;

pub use levels::{db_to_linear, linear_to_db, ClippingProtection, LoudnessMatch, PeakMeter};
pub use pan::{PanLaw, PanMatrix, StereoPan};

/// One recording in the mix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixTrack {
    /// Which recording of the alignment report this track is.
    ///
    /// The index refers to [`AlignmentReport::sources`]. The reference is **not**
    /// among them: it is never a track of a mix.
    pub source_index: usize,

    /// The fader, as a linear gain.
    ///
    /// A gain of one is the recording's own level. The gain is applied to every
    /// channel of the recording, so a stereo recording's two channels stay in step.
    pub gain: f32,

    /// The trim, applied before the fader.
    ///
    /// The trim is there for the recordings that need a large correction — a
    /// microphone far from the stage, say — and it is kept separate from the fader
    /// so that moving the fader does not throw the trim away.
    pub trim: f32,

    /// The pan position: `-1` is hard left, `+1` is hard right.
    pub pan: f32,

    /// The law used to pan a mono recording.
    pub pan_law: PanLaw,

    /// How a stereo recording is panned.
    pub stereo_pan: StereoPan,

    /// Whether the recording is muted.
    pub mute: bool,

    /// Whether the recording is soloed.
    ///
    /// When any recording is soloed, the recordings that are not are silent — the
    /// ordinary meaning of a solo button.
    pub solo: bool,

    /// Whether the recording contributes to the mix at all.
    pub enabled: bool,
}

impl Default for MixTrack {
    fn default() -> Self {
        Self {
            source_index: 0,
            gain: 1.0,
            trim: 1.0,
            pan: 0.0,
            pan_law: PanLaw::default(),
            stereo_pan: StereoPan::default(),
            mute: false,
            solo: false,
            enabled: true,
        }
    }
}

impl MixTrack {
    /// A track for one recording, at its own level and in the centre.
    pub fn new(source_index: usize) -> Self {
        Self {
            source_index,
            ..Self::default()
        }
    }

    /// The gain the recording contributes with: the trim and the fader together.
    ///
    /// A muted recording contributes nothing at all, and a recording that is not
    /// soloed while another one is contributes nothing either.
    pub fn effective_gain(&self, solo_active: bool) -> f32 {
        if !self.enabled || self.mute {
            return 0.0;
        }
        if solo_active && !self.solo {
            return 0.0;
        }
        self.trim * self.gain
    }
}

/// The whole mix.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MixSettings {
    /// The tracks, in the order they are listed.
    pub tracks: Vec<MixTrack>,
    /// The master fader, as a linear gain.
    pub master_gain: f32,
}

impl MixSettings {
    /// A mix of the given recordings, all at their own level and in the centre.
    pub fn of_sources(source_indices: &[usize]) -> Self {
        Self {
            tracks: source_indices.iter().copied().map(MixTrack::new).collect(),
            master_gain: 1.0,
        }
    }

    /// The track of one recording, when there is one.
    pub fn track_for(&self, source_index: usize) -> Option<&MixTrack> {
        self.tracks.iter().find(|t| t.source_index == source_index)
    }

    /// The track of one recording, for change.
    pub fn track_for_mut(&mut self, source_index: usize) -> Option<&mut MixTrack> {
        self.tracks
            .iter_mut()
            .find(|t| t.source_index == source_index)
    }

    /// Whether any recording is soloed.
    pub fn solo_active(&self) -> bool {
        self.tracks.iter().any(|t| t.enabled && t.solo)
    }

    /// The recordings that a monitoring mode plays.
    ///
    /// The reference is never one of them: it is monitored on its own, and it is
    /// never mixed with the recordings that were aligned to it.
    pub fn for_mode(mode: MonitorMode, tracks: &[MixTrack]) -> Vec<MixTrack> {
        let audible: Vec<MixTrack> = tracks.iter().filter(|t| t.enabled).cloned().collect();
        match mode {
            // The reference is played on its own and is not part of the mix.
            MonitorMode::ReferenceOnly => Vec::new(),
            MonitorMode::TargetOnly | MonitorMode::AbSwitch => {
                audible.into_iter().take(1).collect()
            }
            MonitorMode::Both | MonitorMode::StereoOverlay => audible,
            MonitorMode::StereoSplit => {
                // The first recording is heard from the left and the second from the
                // right. A recording that has two channels is folded to one, because
                // an image cannot be placed in a single speaker.
                let mut split: Vec<MixTrack> = audible
                    .into_iter()
                    .take(2)
                    .map(|mut t| {
                        t.stereo_pan = StereoPan::MonoFold;
                        t
                    })
                    .collect();
                if let Some(first) = split.first_mut() {
                    first.pan = -1.0;
                }
                if let Some(second) = split.get_mut(1) {
                    second.pan = 1.0;
                }
                split
            }
        }
    }
}

/// What is being listened to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MonitorMode {
    /// The reference recording alone.
    #[default]
    ReferenceOnly,

    /// The selected recording alone, with its correction applied.
    TargetOnly,

    /// The reference and the selected recording, switched by hand.
    ///
    /// The switch happens at the same position on the timeline, so the two are
    /// compared like for like.
    AbSwitch,

    /// Every recording at once, mixed.
    Both,

    /// The first recording from the left and the second from the right.
    StereoSplit,

    /// Every recording at once, each keeping its own channels.
    ///
    /// This is the mode for two stereo microphones of the same performance: both
    /// images are heard together, and neither is folded into the other.
    StereoOverlay,
}

impl MonitorMode {
    /// Every mode, in the order the interface lists them.
    pub const ALL: [MonitorMode; 6] = [
        MonitorMode::ReferenceOnly,
        MonitorMode::TargetOnly,
        MonitorMode::AbSwitch,
        MonitorMode::Both,
        MonitorMode::StereoSplit,
        MonitorMode::StereoOverlay,
    ];

    /// The text shown for the mode.
    pub fn label(self) -> &'static str {
        match self {
            MonitorMode::ReferenceOnly => "Reference only",
            MonitorMode::TargetOnly => "Target only",
            MonitorMode::AbSwitch => "A/B switch",
            MonitorMode::Both => "All together",
            MonitorMode::StereoSplit => "Left / right split",
            MonitorMode::StereoOverlay => "Stereo overlay",
        }
    }

    /// A sentence saying what the mode does.
    pub fn description(self) -> &'static str {
        match self {
            MonitorMode::ReferenceOnly => "Only the reference recording is heard.",
            MonitorMode::TargetOnly => "Only the selected recording is heard, corrected.",
            MonitorMode::AbSwitch => "The reference and the selected recording, switched by hand.",
            MonitorMode::Both => "Every recording is heard at once, mixed.",
            MonitorMode::StereoSplit => {
                "The first recording is heard from the left and the second from the right."
            }
            MonitorMode::StereoOverlay => {
                "Every recording is heard at once and keeps its own left and right channels."
            }
        }
    }
}

/// The container the mix is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ContainerChoice {
    /// The smallest container that can hold the file.
    ///
    /// An ordinary RIFF/WAVE file is used while the payload fits in it, and an RF64
    /// file is used when it does not.
    #[default]
    Auto,
    /// An ordinary RIFF/WAVE file.
    ///
    /// A payload that does not fit is refused rather than written with a size field
    /// that has wrapped.
    Riff,
    /// An RF64 file.
    Rf64,
    /// A BW64 file: RF64 with a broadcast-extension chunk.
    Bw64,
}

/// How the mix is written.
#[derive(Debug, Clone)]
pub struct MixOptions {
    /// The output sample rate; `None` means the reference's own rate.
    pub sample_rate: Option<u32>,
    /// The number of output channels: one or two.
    ///
    /// A mono mixdown ignores the pan, because a pan control has nothing to do when
    /// there is only one output channel.
    pub channels: usize,
    /// The output sample format.
    pub format: WavFormat,
    /// Resampler taps.
    pub taps: usize,
    /// Resampler phases.
    pub phases: usize,
    /// Frames per decoded block.
    pub block_frames: usize,
    /// How the mix protects itself against clipping.
    pub protection: ClippingProtection,
    /// Whether the recordings are matched to a common level before they are mixed.
    pub loudness: LoudnessMatch,
    /// The container the file is written in.
    pub container: ContainerChoice,
}

impl Default for MixOptions {
    fn default() -> Self {
        Self {
            sample_rate: None,
            channels: 2,
            format: WavFormat::Float32,
            taps: crate::resample::DEFAULT_TAPS,
            phases: crate::resample::DEFAULT_PHASES,
            block_frames: 1 << 16,
            protection: ClippingProtection::None,
            loudness: LoudnessMatch::None,
            container: ContainerChoice::Auto,
        }
    }
}

/// The peak of one recording, after its own fader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackPeak {
    /// Which recording of the alignment report this is.
    pub source_index: usize,
    /// The loudest sample it contributed.
    pub peak: f32,
    /// How many samples it contributed above 0 dBFS.
    pub overs: u64,
}

/// What a mix produced.
#[derive(Debug, Clone)]
pub struct MixResult {
    /// The path of the written file.
    pub path: PathBuf,
    /// The output sample rate.
    pub sample_rate: u32,
    /// The number of output channels.
    pub channels: usize,
    /// The number of frames written.
    pub frames: u64,
    /// The loudest sample of the mix **before** any protection was applied.
    ///
    /// This is the level the user asked for. The level that was written is
    /// [`MixResult::peak`].
    pub mix_peak: f32,
    /// The loudest sample that was written.
    pub peak: f32,
    /// How many samples of the written file are above 0 dBFS.
    pub overs: u64,
    /// The gain the protection applied, in decibels. Negative means it was reduced.
    pub gain_reduction_db: f64,
    /// How many samples the soft clipper altered.
    pub clipped_samples: u64,
    /// The peak of each recording.
    pub track_peaks: Vec<TrackPeak>,
    /// The container the file was written in.
    pub container: Container,
    /// The protection that was applied.
    pub protection: ClippingProtection,
    /// The loudness gain that was applied to each recording, in decibels.
    pub loudness_gains_db: Vec<f64>,
    /// Notes worth telling the user about.
    pub notes: Vec<String>,
}

impl MixResult {
    /// The loudest sample of the written file, in dBFS.
    pub fn peak_dbfs(&self) -> f64 {
        linear_to_db(self.peak)
    }

    /// The loudest sample of the mix before the protection, in dBFS.
    pub fn mix_peak_dbfs(&self) -> f64 {
        linear_to_db(self.mix_peak)
    }
}

/// The result of one pass over the material.
struct PassResult {
    /// The loudest sample of the mix before the master gain.
    mix_peak: f32,
    /// The loudest sample that was written.
    peak: f32,
    /// How many written samples are above 0 dBFS.
    overs: u64,
    /// How many samples the soft clipper altered.
    clipped: u64,
    /// The peak of each recording.
    track_peaks: Vec<TrackPeak>,
    /// The number of frames written.
    frames: u64,
    /// Notes raised while the recordings were opened.
    notes: Vec<String>,
}

/// One recording, read through its position map and ready to be mixed.
struct TrackReader {
    /// Which recording of the alignment report this is.
    source_index: usize,
    /// The cursor over the recording's material.
    cursor: SourceCursor,
    /// The matrix that places the recording in the field.
    matrix: PanMatrix,
    /// The gain the recording contributes with.
    gain: f32,
    /// The meter of the recording.
    meter: PeakMeter,
}

/// The index of the recording a name refers to.
///
/// The name may be the recording's file name or its full path. A bare file name is
/// **not** an identity — two recordings can share one, and a mix that silently
/// picked the wrong one of them would be worse than a refusal — so a name that
/// matches more than one recording is an error rather than a guess.
pub fn resolve_source(report: &AlignmentReport, name: &str) -> Result<usize> {
    let wanted = std::path::Path::new(name);
    let mut matches: Vec<usize> = Vec::new();
    for (index, alignment) in report.sources.iter().enumerate() {
        let path = &alignment.source.path;
        let same_path = path == wanted
            || path
                .file_name()
                .map(|n| n.to_string_lossy() == name)
                .unwrap_or(false);
        if same_path {
            matches.push(index);
        }
    }
    match matches.len() {
        0 => anyhow::bail!(
            "no recording named \"{name}\" is part of this analysis. The recordings are: {}",
            report
                .sources
                .iter()
                .map(|a| a.source.file_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        1 => Ok(matches[0]),
        _ => anyhow::bail!(
            "\"{name}\" names more than one recording. Use the full path to say which one \
             you mean."
        ),
    }
}

/// The root-mean-square level of a recording, in dBFS.
///
/// This is an RMS measurement over the whole recording, not a perceptual loudness
/// measurement: it applies no frequency weighting.
pub fn measure_rms_dbfs(path: &Path) -> Result<f64> {
    let mut decoder = AudioDecoder::open(path)?;
    let mut sum = 0f64;
    let mut count = 0f64;
    decoder.for_each_block(1 << 16, |block| {
        for plane in &block.planes {
            for sample in plane {
                sum += (*sample as f64) * (*sample as f64);
                count += 1.0;
            }
        }
        Ok(())
    })?;
    if count == 0.0 {
        return Ok(f64::NEG_INFINITY);
    }
    Ok(linear_to_db((sum / count).sqrt() as f32))
}

/// The container a payload of this size is written in.
///
/// An ordinary RIFF file cannot describe a payload of more than four gibibytes, so a
/// bigger one is written as RF64 — unless the caller asked for RIFF outright, in
/// which case the export is refused rather than written with a size field that has
/// wrapped.
pub fn choose_container(
    choice: ContainerChoice,
    payload_bytes: u64,
) -> Result<(Container, Option<String>)> {
    match choice {
        ContainerChoice::Auto => {
            if payload_bytes <= crate::audio::RIFF_MAX_DATA_BYTES {
                Ok((Container::Riff, None))
            } else {
                Ok((
                    Container::Rf64,
                    Some(format!(
                        "the mix needs {payload_bytes} bytes of audio, which is more than an \
                         ordinary RIFF/WAVE file can describe, so it was written as RF64"
                    )),
                ))
            }
        }
        ContainerChoice::Riff => {
            if payload_bytes > crate::audio::RIFF_MAX_DATA_BYTES {
                anyhow::bail!(
                    "the mix needs {payload_bytes} bytes of audio, which is more than an \
                     ordinary RIFF/WAVE file can describe ({} bytes). Write it in the RF64 \
                     container instead.",
                    crate::audio::RIFF_MAX_DATA_BYTES
                );
            }
            Ok((Container::Riff, None))
        }
        ContainerChoice::Rf64 => Ok((Container::Rf64, None)),
        ContainerChoice::Bw64 => Ok((Container::Bw64, None)),
    }
}

/// Measure the level of every recording, and say how far each one has to be moved.
fn loudness_gains(
    report: &AlignmentReport,
    settings: &MixSettings,
    loudness: LoudnessMatch,
) -> Result<(Vec<f64>, Vec<String>)> {
    let mut gains = vec![0.0f64; settings.tracks.len()];
    let mut notes = Vec::new();
    if loudness == LoudnessMatch::None {
        return Ok((gains, notes));
    }
    let mut levels: Vec<(usize, f64)> = Vec::new();
    for track in &settings.tracks {
        let Some(alignment) = report.sources.get(track.source_index) else {
            continue;
        };
        let level = measure_rms_dbfs(&alignment.source.path)
            .with_context(|| format!("failed to measure {}", alignment.source.file_name))?;
        levels.push((track.source_index, level));
    }
    let loudest = levels
        .iter()
        .map(|(_, level)| *level)
        .fold(f64::NEG_INFINITY, f64::max);
    if !loudest.is_finite() {
        notes
            .push("the loudness match could not be applied: the recordings are silent".to_string());
        return Ok((gains, notes));
    }
    for (index, track) in settings.tracks.iter().enumerate() {
        let Some((_, level)) = levels.iter().find(|(i, _)| *i == track.source_index) else {
            continue;
        };
        let gain_db = loudest - level;
        gains[index] = gain_db;
        if gain_db.abs() > 0.01 {
            notes.push(format!(
                "the RMS loudness match moved recording {} by {gain_db:+.2} dB",
                track.source_index + 1
            ));
        }
    }
    Ok((gains, notes))
}

/// Open the recordings of the mix and place them on the reference timeline.
fn open_tracks(
    report: &AlignmentReport,
    settings: &MixSettings,
    options: &MixOptions,
    output_rate: u32,
    total_frames: u64,
    loudness: &[f64],
) -> Result<(Vec<TrackReader>, Vec<String>)> {
    let mut notes = Vec::new();
    let mut readers = Vec::new();
    let solo_active = settings.solo_active();
    let output_channels = options.channels.clamp(1, 2);

    for (index, track) in settings.tracks.iter().enumerate() {
        let gain = track.effective_gain(solo_active) * db_to_linear(loudness[index]);
        if gain == 0.0 {
            // A muted, disabled or un-soloed recording is not opened at all: there is
            // no reason to read a file whose samples will be multiplied by zero.
            continue;
        }
        let Some(alignment) = report.sources.get(track.source_index) else {
            notes.push(format!(
                "recording {} is not in the analysis, so it contributes nothing",
                track.source_index + 1
            ));
            continue;
        };
        let Some(model) = alignment.model() else {
            notes.push(format!(
                "{} has no reliable alignment, so it contributes nothing",
                alignment.source.file_name
            ));
            continue;
        };
        let decoder = AudioDecoder::open(&alignment.source.path).with_context(|| {
            format!("failed to open {} for the mix", alignment.source.file_name)
        })?;
        let source_rate = decoder.meta().sample_rate;
        let source_channels = decoder.meta().channels.max(1);
        if source_channels > 2 {
            anyhow::bail!(
                "{} has {} channels, and the mixer places a recording with at most two. \
                 Export the recording on its own instead.",
                alignment.source.file_name,
                source_channels
            );
        }
        let geometry = geometry_timeline(&model, source_rate, output_rate);

        // The span of the reference timeline the recording covers.
        let content_start = ((model.offset) * output_rate as f64)
            .round()
            .clamp(0.0, total_frames as f64) as u64;
        let content_end = (((model.offset + model.scale * alignment.source.duration_secs)
            * output_rate as f64)
            .round()
            .clamp(0.0, total_frames as f64)) as u64;
        let content_end = content_end.max(content_start);
        let content_frames = content_end - content_start;

        let matrix = if output_channels == 1 {
            PanMatrix::fold(source_channels)
        } else {
            PanMatrix::for_source(source_channels, track.pan, track.pan_law, track.stereo_pan)
        };

        let cursor = SourceCursor::new(
            decoder,
            geometry,
            content_start as i64,
            Some(content_frames),
            options.taps,
            options.phases,
            options.block_frames,
        );
        readers.push(TrackReader {
            source_index: track.source_index,
            cursor,
            matrix,
            gain,
            meter: PeakMeter::new(),
        });
    }
    Ok((readers, notes))
}

/// Run one pass over the material, either measuring it or writing it.
///
/// The two are the same arithmetic: a pass that writes is a pass that measures with
/// a file at the end of it. That is what makes the automatic gain honest — the peak
/// it works from is the peak of the mix that was actually written.
#[allow(clippy::too_many_arguments)]
fn mix_pass(
    report: &AlignmentReport,
    settings: &MixSettings,
    options: &MixOptions,
    output_rate: u32,
    total_frames: u64,
    loudness: &[f64],
    master_gain: f32,
    container: Container,
    out_path: Option<&Path>,
) -> Result<PassResult> {
    let (mut readers, notes) = open_tracks(
        report,
        settings,
        options,
        output_rate,
        total_frames,
        loudness,
    )?;
    let output_channels = options.channels.clamp(1, 2);

    let mut writer = match out_path {
        Some(path) => Some(WavWriter::create_with_container(
            path,
            output_rate,
            output_channels,
            options.format,
            container,
        )?),
        None => None,
    };

    let block = options.block_frames.max(1024);
    let mut left = vec![0f32; block];
    let mut right = vec![0f32; block];

    let mut mix_peak = 0f32;
    let mut out_peak = 0f32;
    let mut overs = 0u64;
    let mut clipped = 0u64;
    let mut written = 0u64;

    let mut start = 0u64;
    while start < total_frames {
        let end = (start + block as u64).min(total_frames);
        let count = (end - start) as usize;
        left[..count].fill(0.0);
        right[..count].fill(0.0);

        for reader in readers.iter_mut() {
            let frames = reader.cursor.take(start as i64, end as i64)?;
            let available = frames.iter().map(|p| p.len()).min().unwrap_or(0);
            let left_plane = frames.first().map(Vec::as_slice).unwrap_or(&[]);
            let right_plane = frames.get(1).map(Vec::as_slice).unwrap_or(&[]);
            for i in 0..available {
                let input = [
                    left_plane.get(i).copied().unwrap_or(0.0),
                    right_plane.get(i).copied().unwrap_or(0.0),
                ];
                let mut out = [0f32; 2];
                reader.matrix.apply(&input, &mut out);
                let contribution_l = out[0] * reader.gain;
                let contribution_r = out[1] * reader.gain;
                left[i] += contribution_l;
                right[i] += contribution_r;
                reader.meter.observe(contribution_l);
                reader.meter.observe(contribution_r);
            }
        }

        // The mix is written in floating point, and the protection is applied here.
        let mut planes: Vec<Vec<f32>> = Vec::with_capacity(output_channels);
        if output_channels == 1 {
            // A mono mixdown ignores the pan: the matrix has already folded the
            // recording into the left channel, so the left channel *is* the mix.
            let mut mono = vec![0f32; count];
            for i in 0..count {
                let raw = left[i];
                if raw.abs() > mix_peak {
                    mix_peak = raw.abs();
                }
                let value = options.protection.apply(raw, master_gain);
                if value.abs() > out_peak {
                    out_peak = value.abs();
                }
                if value.abs() > 1.0 {
                    overs += 1;
                }
                if (value - raw * master_gain).abs() > 1e-9 {
                    clipped += 1;
                }
                mono[i] = value;
            }
            planes.push(mono);
        } else {
            let mut plane_l = vec![0f32; count];
            let mut plane_r = vec![0f32; count];
            for i in 0..count {
                let raw_l = left[i];
                let raw_r = right[i];
                if raw_l.abs() > mix_peak {
                    mix_peak = raw_l.abs();
                }
                if raw_r.abs() > mix_peak {
                    mix_peak = raw_r.abs();
                }
                let value_l = options.protection.apply(raw_l, master_gain);
                let value_r = options.protection.apply(raw_r, master_gain);
                if value_l.abs() > out_peak {
                    out_peak = value_l.abs();
                }
                if value_r.abs() > out_peak {
                    out_peak = value_r.abs();
                }
                if value_l.abs() > 1.0 {
                    overs += 1;
                }
                if value_r.abs() > 1.0 {
                    overs += 1;
                }
                if (value_l - raw_l * master_gain).abs() > 1e-9 {
                    clipped += 1;
                }
                if (value_r - raw_r * master_gain).abs() > 1e-9 {
                    clipped += 1;
                }
                plane_l[i] = value_l;
                plane_r[i] = value_r;
            }
            planes.push(plane_l);
            planes.push(plane_r);
        }

        if let Some(writer) = writer.as_mut() {
            writer.write_planar(&planes)?;
        }
        written += count as u64;
        start = end;
    }

    if let Some(writer) = writer {
        writer.finalize()?;
    }

    let track_peaks = readers
        .iter()
        .map(|reader| TrackPeak {
            source_index: reader.source_index,
            peak: reader.meter.peak,
            overs: reader.meter.overs,
        })
        .collect();

    Ok(PassResult {
        mix_peak,
        peak: out_peak,
        overs,
        clipped,
        track_peaks,
        frames: written,
        notes,
    })
}

/// Mix the recordings and write the result.
///
/// The mix is placed on the **reference timeline**: every recording is at the
/// position its alignment says it belongs at, and the output is exactly as long as
/// the reference.
pub fn render_mix(
    report: &AlignmentReport,
    settings: &MixSettings,
    options: &MixOptions,
    out_path: &Path,
) -> Result<MixResult> {
    crate::render::ensure_output_does_not_overwrite(out_path, &[])?;
    let mut inputs: Vec<PathBuf> = vec![report.reference.path.clone()];
    for alignment in &report.sources {
        inputs.push(alignment.source.path.clone());
    }
    crate::render::ensure_output_does_not_overwrite(out_path, &inputs)?;
    let output_rate = options
        .sample_rate
        .unwrap_or(report.reference.sample_rate)
        .max(1);
    let output_channels = options.channels.clamp(1, 2);
    let total_frames = (report.timeline_duration * output_rate as f64)
        .round()
        .max(0.0) as u64;

    let (loudness, mut notes) = loudness_gains(report, settings, options.loudness)?;

    // The container is chosen from the size the mix will have, so the decision is
    // made before a single sample is written.
    let payload_bytes = total_frames
        .saturating_mul(output_channels as u64)
        .saturating_mul(options.format.bytes_per_sample());
    let (container, container_note) = choose_container(options.container, payload_bytes)?;
    if let Some(note) = container_note {
        notes.push(note);
    }

    // The automatic gain needs the peak of the mix before it can apply it, so the
    // material is measured first. The measurement is the same arithmetic as the
    // write, so the peak it works from is the peak of the file that is written.
    let (master_gain, measured) = match options.protection {
        ClippingProtection::AutoGain { .. } => {
            let measured = mix_pass(
                report,
                settings,
                options,
                output_rate,
                total_frames,
                &loudness,
                1.0,
                container,
                None,
            )?;
            let gain = options.protection.master_gain(measured.mix_peak);
            let reduction_db = -options.protection.gain_reduction_db(measured.mix_peak);
            if reduction_db > 0.0 {
                notes.push(format!(
                    "the peak of the mix was {:.2} dBFS; {:.2} dB of gain reduction was \
                     applied to bring it to the {:.2} dBFS target",
                    linear_to_db(measured.mix_peak),
                    reduction_db,
                    options.protection.target_dbfs()
                ));
            } else {
                // The protection only ever turns the mix down. A mix that is already
                // below the target is left alone, so saying "gain was applied" here
                // would be false.
                notes.push(format!(
                    "the peak of the mix was {:.2} dBFS, below the {:.2} dBFS target, so no \
                     gain reduction was needed",
                    linear_to_db(measured.mix_peak),
                    options.protection.target_dbfs()
                ));
            }
            (gain, Some(measured))
        }
        _ => (1.0, None),
    };

    let written = mix_pass(
        report,
        settings,
        options,
        output_rate,
        total_frames,
        &loudness,
        master_gain,
        container,
        Some(out_path),
    )?;

    let gain_reduction_db = match &measured {
        Some(measured) => options.protection.gain_reduction_db(measured.mix_peak),
        None => options.protection.gain_reduction_db(written.mix_peak),
    };
    notes.extend(written.notes.clone());

    if written.overs > 0 {
        if options.format == WavFormat::Float32 {
            notes.push(format!(
                "{} samples of the mix are above 0 dBFS. They are written as they are, \
                 because a 32-bit float file can hold them; turn the faders down if they \
                 are not wanted.",
                written.overs
            ));
        } else {
            notes.push(format!(
                "{} samples of the mix are above 0 dBFS and were clipped by the 16-bit \
                 format. Turn the faders down and export again to keep them.",
                written.overs
            ));
        }
    }
    if written.clipped > 0 {
        notes.push(format!(
            "the soft clipper changed {} samples",
            written.clipped
        ));
    }
    if settings.tracks.is_empty() {
        notes.push("no recording was chosen for the mix, so the file is silent".to_string());
    }

    Ok(MixResult {
        path: out_path.to_path_buf(),
        sample_rate: output_rate,
        channels: output_channels,
        frames: written.frames,
        mix_peak: written.mix_peak,
        peak: written.peak,
        overs: written.overs,
        gain_reduction_db,
        clipped_samples: written.clipped,
        track_peaks: written.track_peaks,
        container,
        protection: options.protection,
        loudness_gains_db: loudness,
        notes,
    })
}
