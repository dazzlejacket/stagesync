//! Rendering: turning a time model into corrected audio.
//!
//! Three layouts are supported:
//!
//! * [`Layout::SourceAligned`] — one file per source, carrying the drift correction and
//!   nothing else. The source's own material is never truncated; its duration changes by
//!   the measured drift, which is the honest consequence of correcting a clock.
//! * [`Layout::ReferenceTimelinePerSource`] — one file per source, placed on the
//!   reference's timeline. Spans no source covers are **digital silence placeholders**,
//!   explicitly labelled as such, never invented audio.
//! * [`Layout::ReferenceTimelineMerged`] — one file covering the whole reference timeline,
//!   with every source placed and overlaps resolved by user priority.
//!
//! Every layout uses the same polyphase resampler and the same exact position map, so the
//! output is deterministic and does not depend on how the input was chunked.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::audio::{AudioDecoder, WavFormat, WavWriter};
use crate::matching::model::LinearModel;
use crate::resample::{StreamingResampler, DEFAULT_PHASES, DEFAULT_TAPS};

/// Output layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Corrected sources, each keeping its own length and sample rate.
    SourceAligned,
    /// One reference-length timeline per source, silence elsewhere.
    ReferenceTimelinePerSource,
    /// One merged reference-length timeline covering all sources.
    ReferenceTimelineMerged,
}

impl Layout {
    /// Short name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            Layout::SourceAligned => "source_aligned",
            Layout::ReferenceTimelinePerSource => "reference_timeline_per_source",
            Layout::ReferenceTimelineMerged => "reference_timeline_merged",
        }
    }
}

/// Rendering options.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Output layout.
    pub layout: Layout,
    /// Output sample rate. `None` means "the source rate" for source-aligned renders and
    /// "the reference rate" for timeline renders.
    pub sample_rate: Option<u32>,
    /// Output sample format.
    pub format: WavFormat,
    /// Resampler taps.
    pub taps: usize,
    /// Resampler phases.
    pub phases: usize,
    /// Frames per decoded block.
    pub block_frames: usize,
    /// Optional leading silence inserted into a source-aligned render, in seconds.
    pub leading_silence_secs: Option<f64>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            layout: Layout::SourceAligned,
            sample_rate: None,
            format: WavFormat::Float32,
            taps: DEFAULT_TAPS,
            phases: DEFAULT_PHASES,
            block_frames: 1 << 16,
            leading_silence_secs: None,
        }
    }
}

/// What a render produced.
#[derive(Debug, Clone)]
pub struct RenderResult {
    /// Path of the written file.
    pub path: PathBuf,
    /// Output sample rate.
    pub sample_rate: u32,
    /// Output channel count.
    pub channels: usize,
    /// Total frames written.
    pub frames: u64,
    /// Frames of real source audio.
    pub source_frames: u64,
    /// Frames of silence placeholder.
    pub silence_frames: u64,
    /// Effective rate factor applied (output frames per source frame).
    pub effective_rate: f64,
    /// Pitch change caused by the resampling, in cents (negative = pitched down).
    pub pitch_shift_cents: f64,
}

/// Source-to-output geometry for one source.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    /// Output frames advanced per source frame.
    alpha: f64,
    /// Source frame position of output frame 0.
    gamma: f64,
}

/// Geometry for a source-aligned render: the source's own material, drift corrected.
fn geometry_source_aligned(model: &LinearModel, source_rate: u32, output_rate: u32) -> Geometry {
    Geometry {
        alpha: source_rate as f64 / (output_rate as f64 * model.scale),
        gamma: 0.0,
    }
}

/// Geometry for a reference-timeline render.
fn geometry_timeline(model: &LinearModel, source_rate: u32, output_rate: u32) -> Geometry {
    Geometry {
        alpha: source_rate as f64 / (output_rate as f64 * model.scale),
        gamma: -model.offset * source_rate as f64 / model.scale,
    }
}

/// A bank of resamplers — one per channel — driven by an identical position map.
///
/// Every channel therefore receives exactly the same time mapping, which preserves the
/// inter-channel relationships (the stereo image) exactly.
struct ChannelBank {
    resamplers: Vec<StreamingResampler>,
}

impl ChannelBank {
    fn new(channels: usize, geom: Geometry, options: &RenderOptions, start_output: i64) -> Self {
        let mut resamplers = Vec::with_capacity(channels);
        for _ in 0..channels {
            let mut r =
                StreamingResampler::new(geom.alpha, geom.gamma, options.taps, options.phases);
            r.set_next_output(start_output);
            resamplers.push(r);
        }
        Self { resamplers }
    }

    fn push(&mut self, planes: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(self.resamplers.len());
        for (r, plane) in self.resamplers.iter_mut().zip(planes.iter()) {
            out.push(r.push(plane));
        }
        let len = out.iter().map(|p| p.len()).max().unwrap_or(0);
        for p in out.iter_mut() {
            p.resize(len, 0.0);
        }
        out
    }

    fn flush(&mut self) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(self.resamplers.len());
        for r in self.resamplers.iter_mut() {
            out.push(r.flush());
        }
        let len = out.iter().map(|p| p.len()).max().unwrap_or(0);
        for p in out.iter_mut() {
            p.resize(len, 0.0);
        }
        out
    }
}

/// Render one source with its drift correction applied, keeping its own timeline.
///
/// The output contains the source's own material only: no leading silence is inserted
/// unless explicitly requested. The placement onto the reference timeline is described by
/// the model and written to the sync map, not by padding the file.
pub fn render_source_aligned(
    source_path: &Path,
    model: &LinearModel,
    out_path: &Path,
    options: &RenderOptions,
) -> Result<RenderResult> {
    ensure_output_does_not_overwrite(out_path, &[source_path.to_path_buf()])?;
    let mut decoder = AudioDecoder::open(source_path)?;
    let meta = decoder.meta().clone();
    let source_rate = meta.sample_rate;
    let output_rate = options.sample_rate.unwrap_or(source_rate);
    let channels = meta.channels.max(1);

    let geom = geometry_source_aligned(model, source_rate, output_rate);
    let mut bank = ChannelBank::new(channels, geom, options, 0);

    let mut writer = WavWriter::create(out_path, output_rate, channels, options.format)?;
    if let Some(lead) = options.leading_silence_secs {
        let frames = (lead * output_rate as f64).round().max(0.0) as usize;
        writer.write_silence(frames)?;
    }

    let mut source_frames = 0u64;
    decoder.for_each_block(options.block_frames, |block| {
        let out = bank.push(&block.planes);
        let n = out.first().map_or(0, |p| p.len());
        if n > 0 {
            writer.write_planar(&out)?;
            source_frames += n as u64;
        }
        Ok(())
    })?;

    let tail = bank.flush();
    let n = tail.first().map_or(0, |p| p.len());
    if n > 0 {
        writer.write_planar(&tail)?;
        source_frames += n as u64;
    }
    let frames = writer.frames_written();
    writer.finalize()?;
    Ok(RenderResult {
        path: out_path.to_path_buf(),
        sample_rate: output_rate,
        channels,
        frames,
        source_frames,
        silence_frames: 0,
        effective_rate: geom.alpha,
        pitch_shift_cents: -1200.0 * model.scale.log2(),
    })
}

/// Render one source placed on the reference timeline.
///
/// Output frame `n` corresponds to reference time `n / output_rate`. Spans that the source
/// does not cover are written as digital silence and are explicitly reported as
/// placeholders.
pub fn render_reference_timeline(
    source_path: &Path,
    model: &LinearModel,
    reference_duration: f64,
    out_path: &Path,
    options: &RenderOptions,
) -> Result<RenderResult> {
    ensure_output_does_not_overwrite(out_path, &[source_path.to_path_buf()])?;
    let mut decoder = AudioDecoder::open(source_path)?;
    let meta = decoder.meta().clone();
    let source_rate = meta.sample_rate;
    let output_rate = options.sample_rate.unwrap_or(source_rate);
    let channels = meta.channels.max(1);

    let geom = geometry_timeline(model, source_rate, output_rate);
    let total_frames = (reference_duration * output_rate as f64).round().max(0.0) as u64;

    // The output frames that carry source content.
    let content_start = ((model.offset) * output_rate as f64)
        .round()
        .clamp(0.0, total_frames as f64) as u64;
    let source_duration = meta.duration_secs;
    let content_end = (((model.offset + model.scale * source_duration) * output_rate as f64)
        .round()
        .clamp(0.0, total_frames as f64)) as u64;
    let content_end = content_end.max(content_start);
    let _ = content_end;

    let mut writer = WavWriter::create(out_path, output_rate, channels, options.format)?;

    // Leading silence placeholder.
    if content_start > 0 {
        writer.write_silence(content_start as usize)?;
    }

    let mut bank = ChannelBank::new(channels, geom, options, content_start as i64);

    let mut source_frames = 0u64;
    decoder.for_each_block(options.block_frames, |block| {
        let out = bank.push(&block.planes);
        let n = out.first().map_or(0, |p| p.len());
        if n > 0 {
            writer.write_planar(&out)?;
            source_frames += n as u64;
        }
        Ok(())
    })?;

    let tail = bank.flush();
    let n = tail.first().map_or(0, |p| p.len());
    if n > 0 {
        writer.write_planar(&tail)?;
        source_frames += n as u64;
    }

    // Trailing silence placeholder.
    let written = writer.frames_written();
    if written < total_frames {
        writer.write_silence((total_frames - written) as usize)?;
    }
    let frames = writer.frames_written();
    writer.finalize()?;
    Ok(RenderResult {
        path: out_path.to_path_buf(),
        sample_rate: output_rate,
        channels,
        frames,
        source_frames: source_frames.min(frames),
        silence_frames: frames.saturating_sub(source_frames),
        effective_rate: geom.alpha,
        pitch_shift_cents: -1200.0 * model.scale.log2(),
    })
}

/// A sequential cursor over one source, used by the merged timeline renderer.
struct SourceCursor {
    decoder: AudioDecoder,
    resampler: StreamingResampler,
    /// Output frames not yet consumed, starting at frame index `pending_start`.
    pending: VecDeque<f32>,
    pending_start: i64,
    finished: bool,
    block_frames: usize,
}

impl SourceCursor {
    fn new(path: &Path, geom: Geometry, start_frame: i64, options: &RenderOptions) -> Result<Self> {
        let decoder = AudioDecoder::open(path)?;
        let mut resampler =
            StreamingResampler::new(geom.alpha, geom.gamma, options.taps, options.phases);
        resampler.set_next_output(start_frame);
        Ok(Self {
            decoder,
            resampler,
            pending: VecDeque::new(),
            pending_start: start_frame,
            finished: false,
            block_frames: options.block_frames,
        })
    }

    /// Ensure output frames up to `target` are available in `pending`.
    fn fill_to(&mut self, target: i64) -> Result<()> {
        while self.pending_start + (self.pending.len() as i64) < target && !self.finished {
            let Some(block) = self.decoder.read_block(self.block_frames)? else {
                // End of stream: flush the resampler and mark the cursor finished.
                let tail = self.resampler.flush();
                self.pending.extend(tail);
                self.finished = true;
                break;
            };
            let mono = block.mono();
            let out = self.resampler.push(&mono);
            self.pending.extend(out);
        }
        Ok(())
    }

    /// Take the frames in `[from, to)`.
    fn take(&mut self, from: i64, to: i64) -> Result<Vec<f32>> {
        self.fill_to(to)?;
        let skip = (from - self.pending_start).max(0) as usize;
        for _ in 0..skip.min(self.pending.len()) {
            self.pending.pop_front();
            self.pending_start += 1;
        }
        let want = (to - from).max(0) as usize;
        let mut out = Vec::with_capacity(want);
        for _ in 0..want {
            match self.pending.pop_front() {
                Some(v) => {
                    out.push(v);
                    self.pending_start += 1;
                }
                None => break,
            }
        }
        Ok(out)
    }
}

/// Render every source onto one merged reference timeline.
///
/// Sources are resolved by priority (the order they were supplied). Overlapping material
/// is taken from the higher-priority source; the other source's material is not deleted
/// from its own per-source export.
pub fn render_merged_timeline(
    sources: &[(PathBuf, LinearModel)],
    reference_duration: f64,
    out_path: &Path,
    options: &RenderOptions,
) -> Result<RenderResult> {
    anyhow::ensure!(!sources.is_empty(), "no sources to render");

    let source_paths: Vec<PathBuf> = sources.iter().map(|(path, _)| path.clone()).collect();
    ensure_output_does_not_overwrite(out_path, &source_paths)?;

    let first_meta = AudioDecoder::open(&sources[0].0)?.meta().clone();
    let output_rate = options.sample_rate.unwrap_or(first_meta.sample_rate);
    let channels = first_meta.channels.max(1);
    let total_frames = (reference_duration * output_rate as f64).round().max(0.0) as u64;

    // Each source's covered output range.
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for (path, model) in sources {
        let meta = AudioDecoder::open(path)?.meta().clone();
        let start = ((model.offset) * output_rate as f64).round().max(0.0) as u64;
        let end = ((model.offset + model.scale * meta.duration_secs) * output_rate as f64)
            .round()
            .max(0.0) as u64;
        ranges.push((start.min(total_frames), end.min(total_frames)));
    }

    let mut writer = WavWriter::create(out_path, output_rate, channels, options.format)?;
    let mut cursors: Vec<Option<SourceCursor>> = (0..sources.len()).map(|_| None).collect();

    let mut cursor = 0u64;
    let mut source_frames = 0u64;
    let mut silence_frames = 0u64;
    let block = (output_rate as u64 / 10).max(1); // ~100 ms blocks

    while cursor < total_frames {
        // The owner is the lowest-index source whose range covers this frame.
        let mut owner: Option<usize> = None;
        for (i, (s, e)) in ranges.iter().copied().enumerate() {
            if cursor >= s && cursor < e {
                owner = Some(i);
                break;
            }
        }

        // The span ends at the earliest of: the end of this block, the end of the
        // owner's range, and the start of the next source.
        //
        // Honouring the exact boundaries matters. A source's material begins and ends
        // where its range says, and a block boundary that happens to fall nearby is
        // not a reason to carry a source's material past the end of its range — that
        // would put a source's resampler ringing into a span that nothing covers, and
        // would lose the first fraction of the next source.
        let mut span_end = (cursor + block).min(total_frames);
        if let Some(i) = owner {
            span_end = span_end.min(ranges[i].1);
        }
        if let Some(next_start) = ranges.iter().map(|(s, _)| *s).filter(|s| *s > cursor).min() {
            span_end = span_end.min(next_start);
        }
        span_end = span_end.max(cursor + 1).min(total_frames);

        match owner {
            Some(i) => {
                let (path, model) = &sources[i];
                let geom = geometry_timeline(model, first_meta.sample_rate, output_rate);
                if cursors[i].is_none() {
                    cursors[i] = Some(SourceCursor::new(path, geom, cursor as i64, options)?);
                }
                let c = cursors[i].as_mut().expect("cursor just created");
                let frames = c.take(cursor as i64, span_end as i64)?;
                let n = frames.len();
                if n > 0 {
                    let per_channel = vec![frames; channels];
                    writer.write_planar(&per_channel)?;
                    source_frames += n as u64;
                }
                let advanced = cursor + n as u64;
                if advanced < span_end {
                    writer.write_silence((span_end - advanced) as usize)?;
                    silence_frames += span_end - advanced;
                }
                cursor = span_end;
            }
            None => {
                writer.write_silence((span_end - cursor) as usize)?;
                silence_frames += span_end - cursor;
                cursor = span_end;
            }
        }
    }

    let frames = writer.frames_written();
    writer.finalize()?;
    Ok(RenderResult {
        path: out_path.to_path_buf(),
        sample_rate: output_rate,
        channels,
        frames,
        source_frames,
        silence_frames,
        effective_rate: 1.0,
        pitch_shift_cents: 0.0,
    })
}

/// Ensure the output directory exists, returning a context for error messages.
pub fn prepare_output(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create output directory: {}", dir.display()))?;
        }
    }
    Ok(())
}

/// Refuse to write an output over one of the recordings that were read.
///
/// The corrected file is named after the recording it comes from, so an export
/// into a directory that already holds a recording of that name lands on the
/// recording itself. The originals are read-only and the reference is never
/// altered at all, so this is refused rather than carried out.
pub fn ensure_output_does_not_overwrite(output: &Path, inputs: &[PathBuf]) -> Result<()> {
    let Some(target) = comparable_path(output) else {
        return Ok(());
    };
    for input in inputs {
        let Some(source) = comparable_path(input) else {
            continue;
        };
        if same_path(&target, &source) {
            anyhow::bail!(
                "refusing to write {}: it would overwrite the recording {}",
                output.display(),
                input.display()
            );
        }
    }
    Ok(())
}

/// Refuse an export whose planned outputs would land on one another.
///
/// The corrected file of a recording is named after the recording, so two
/// recordings that share a file name — the same track captured by two recorders
/// into different folders, say — are given the same output name. The second
/// write would silently replace the first, and one of the two exports would be
/// lost without a word. The export is refused instead, and the message names
/// the file that would have been written twice.
pub fn ensure_outputs_are_distinct(outputs: &[PathBuf]) -> Result<()> {
    for (index, output) in outputs.iter().enumerate() {
        let Some(target) = comparable_path(output) else {
            continue;
        };
        for other in &outputs[index + 1..] {
            let Some(source) = comparable_path(other) else {
                continue;
            };
            if same_path(&target, &source) {
                anyhow::bail!(
                    "refusing to export: {} would be written twice, because two of the \
                     recordings have the same file name. Rename one of them, or export \
                     them one at a time.",
                    output.display()
                );
            }
        }
    }
    Ok(())
}

/// A path in a form that can be compared with another path.
///
/// The output usually does not exist yet, so its parent directory is resolved
/// and the file name is appended to it. Both sides then carry the same
/// directory prefix, which is what makes the comparison reliable.
fn comparable_path(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let parent = absolute.parent()?;
    let name = absolute.file_name()?;
    let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    Some(parent.join(name))
}

/// Whether two paths name the same file.
///
/// Windows paths are compared without regard to case, because the file system
/// does not treat `Soundboard.WAV` and `soundboard.wav` as two files.
#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(not(windows))]
fn same_path(a: &Path, b: &Path) -> bool {
    a == b
}

/// A file name without extension, safe to use as an output name.
///
/// The path separators are removed, because the name is joined to a directory
/// and a separator in it would send the file somewhere else, and whitespace
/// becomes an underscore so that the name can be typed on a command line.
pub fn safe_stem(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| name.to_string());
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect();
    // A name made only of dots would still point at a directory rather than
    // at a file, so it is replaced outright.
    if cleaned.trim().is_empty() || cleaned.chars().all(|c| c == '.') {
        "source".to_string()
    } else {
        cleaned
    }
}

/// The files an export writes for the given layout.
///
/// The names are worked out before anything is written, so that an export
/// which would land on one of the recordings can be refused before the first
/// file is touched.
pub fn planned_outputs(
    out: &Path,
    report: &crate::matching::AlignmentReport,
    layout: Layout,
) -> Vec<PathBuf> {
    match layout {
        Layout::SourceAligned => report
            .sources
            .iter()
            .map(|a| out.join(format!("{}_corrected.wav", safe_stem(&a.source.file_name))))
            .collect(),
        Layout::ReferenceTimelinePerSource => report
            .sources
            .iter()
            .map(|a| out.join(format!("{}_timeline.wav", safe_stem(&a.source.file_name))))
            .collect(),
        Layout::ReferenceTimelineMerged => vec![out.join("timeline_merged.wav")],
    }
}
