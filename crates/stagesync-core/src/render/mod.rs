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
//!   with every source placed and overlaps resolved by the *same* ownership plan the
//!   timeline display and the gap report use.
//!
//! Every layout uses the same polyphase resampler and the same exact position map, so the
//! output is deterministic and does not depend on how the input was chunked.
//!
//! Three rules govern what may be written, and they are checked rather than assumed:
//!
//! * **Every channel is carried.** The channels of a source are never averaged into one
//!   and never duplicated. A stereo source whose two channels are in antiphase keeps
//!   that antiphase; folding it to mono would turn real content into silence.
//! * **Every source is read at its own rate.** A 16 kHz source placed in an 8 kHz timeline
//!   is decimated from 16 kHz, not read as if it were 8 kHz material.
//! * **Every length is exact.** A reference-timeline file has exactly the reference's
//!   duration; a source-aligned file has exactly the length the rate change implies. The
//!   filter's tail is never written past the end.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::audio::{ensure_wav_size_fits, AudioDecoder, WavFormat, WavWriter};
use crate::matching::model::LinearModel;
use crate::matching::{AlignmentReport, SegmentKind};
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
    /// Output channel count, for the merged layout only.
    ///
    /// `None` means "the largest channel count among the sources". A count *smaller*
    /// than a source's own channel count is refused rather than honoured by dropping
    /// channels: losing a channel is losing audio, and silently losing it is worse than
    /// refusing the export.
    pub channels: Option<usize>,
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
            channels: None,
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
    /// Whether the material was resampled.
    ///
    /// `false` means the output was copied sample for sample, which happens when the
    /// correction needs no rate change at all. The distinction matters: a resampled
    /// file has been through a filter, and saying so is part of being honest about it.
    pub resampled: bool,
}

/// Source-to-output geometry for one source.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    /// Output frames advanced per source frame.
    alpha: f64,
    /// Source frame position of output frame 0.
    gamma: f64,
}

impl Geometry {
    /// Whether the map needs no rate change and lands on whole input samples.
    ///
    /// Such a map can be honoured by copying samples instead of filtering them, which
    /// leaves the material untouched.
    fn is_unity_copy(&self) -> bool {
        (self.alpha - 1.0).abs() <= 1e-12 && (self.gamma - self.gamma.round()).abs() <= 1e-9
    }
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

/// The number of output frames that carry material from a source of `source_frames`.
///
/// Output frame `n` is taken from source position `alpha * n + gamma`, so the frames
/// that lie inside the source are those with `0 <= alpha * n + gamma < source_frames`.
/// The count is exact: nothing is written for a position past the end of the source,
/// and nothing is invented for a position before its start.
fn output_frames_for(source_frames: u64, geom: Geometry) -> u64 {
    if source_frames == 0 || geom.alpha <= 0.0 {
        return 0;
    }
    let n = source_frames as f64;
    let first = (-geom.gamma / geom.alpha).ceil().max(0.0);
    let last = ((n - geom.gamma) / geom.alpha).ceil().max(0.0);
    (last - first).max(0.0).round() as u64
}

/// An exact sample copy: output frame `n` is input frame `n + shift`.
///
/// This is used when the map needs no rate change and lands on whole samples. The
/// samples are copied untouched — no filter, no interpolation, no ringing — which is
/// what "an offset-only correction performs no resampling" has to mean.
struct SampleShifter {
    shift: i64,
    buffer: VecDeque<f32>,
    /// Absolute index of the first buffered sample.
    buffer_start: i64,
    next_output: i64,
    produced: u64,
    output_limit: Option<u64>,
    input_end: i64,
}

impl SampleShifter {
    fn new(shift: i64) -> Self {
        Self {
            shift,
            buffer: VecDeque::new(),
            buffer_start: 0,
            next_output: 0,
            produced: 0,
            output_limit: None,
            input_end: 0,
        }
    }

    fn set_next_output(&mut self, n: i64) {
        self.next_output = n;
    }

    fn set_output_limit(&mut self, n: u64) {
        self.output_limit = Some(n);
    }

    fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        self.buffer.extend(samples.iter().copied());
        self.input_end += samples.len() as i64;
        self.drain()
    }

    fn flush(&mut self) -> Vec<f32> {
        self.drain()
    }

    fn drain(&mut self) -> Vec<f32> {
        let mut out = Vec::new();
        loop {
            if let Some(limit) = self.output_limit {
                if self.produced >= limit {
                    break;
                }
            }
            let position = self.next_output + self.shift;
            let index = position - self.buffer_start;
            if index < 0 {
                // The input frame lies before the start of the recording: there is
                // nothing there, and the output frame is silence.
                out.push(0.0);
                self.next_output += 1;
                self.produced += 1;
                continue;
            }
            if (index as usize) >= self.buffer.len() {
                // The input frame has not arrived yet — or, at the end of the stream,
                // will never arrive. An exact copy has no filter tail, so there is
                // nothing more to produce.
                break;
            }
            out.push(self.buffer[index as usize]);
            self.next_output += 1;
            self.produced += 1;
        }

        // Drop the input that can no longer be needed. The origin advances only by the
        // samples that were actually removed.
        let needed = self.next_output + self.shift;
        let drop = (needed - self.buffer_start).max(0) as usize;
        let removed = drop.min(self.buffer.len());
        self.buffer.drain(..removed);
        self.buffer_start += removed as i64;
        out
    }
}

/// One channel's position map: either a resampler or an exact copy.
enum Processor {
    /// Interpolated from input position `alpha * n + gamma`.
    Resample(StreamingResampler),
    /// Copied from input frame `n + shift`.
    Copy(SampleShifter),
}

impl Processor {
    fn new(geom: Geometry, options: &RenderOptions, start_output: i64, limit: Option<u64>) -> Self {
        if geom.is_unity_copy() {
            let mut shifter = SampleShifter::new(geom.gamma.round() as i64);
            shifter.set_next_output(start_output);
            if let Some(limit) = limit {
                shifter.set_output_limit(limit);
            }
            Processor::Copy(shifter)
        } else {
            let mut resampler =
                StreamingResampler::new(geom.alpha, geom.gamma, options.taps, options.phases);
            resampler.set_next_output(start_output);
            if let Some(limit) = limit {
                resampler.set_output_limit(limit);
            }
            Processor::Resample(resampler)
        }
    }

    fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        match self {
            Processor::Resample(r) => r.push(samples),
            Processor::Copy(c) => c.push(samples),
        }
    }

    fn flush(&mut self) -> Vec<f32> {
        match self {
            Processor::Resample(r) => r.flush(),
            Processor::Copy(c) => c.flush(),
        }
    }
}

/// A bank of processors — one per channel — driven by an identical position map.
///
/// Every channel therefore receives exactly the same time mapping, which preserves the
/// inter-channel relationships (the stereo image) exactly.
struct ChannelBank {
    processors: Vec<Processor>,
    /// Whether the bank resamples at all.
    resampled: bool,
}

impl ChannelBank {
    fn new(
        channels: usize,
        geom: Geometry,
        options: &RenderOptions,
        start_output: i64,
        limit: Option<u64>,
    ) -> Self {
        let processors = (0..channels)
            .map(|_| Processor::new(geom, options, start_output, limit))
            .collect();
        let resampled = !geom.is_unity_copy();
        Self {
            processors,
            resampled,
        }
    }

    fn push(&mut self, planes: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(self.processors.len());
        for (processor, plane) in self.processors.iter_mut().zip(planes.iter()) {
            out.push(processor.push(plane));
        }
        let len = out.iter().map(|p| p.len()).max().unwrap_or(0);
        for p in out.iter_mut() {
            p.resize(len, 0.0);
        }
        out
    }

    fn flush(&mut self) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(self.processors.len());
        for processor in self.processors.iter_mut() {
            out.push(processor.flush());
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
///
/// The output has exactly the length the rate change implies. When the correction needs
/// no rate change at all, the samples are copied rather than filtered.
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

    // The length is not known before the source is decoded, so the guard is applied
    // against the longest output the source could possibly produce.
    let max_frames = ((meta.duration_secs * output_rate as f64 * model.scale).ceil() as u64) + 2;
    ensure_wav_size_fits(max_frames, channels, options.format)?;

    let mut writer = WavWriter::create(out_path, output_rate, channels, options.format)?;
    if let Some(lead) = options.leading_silence_secs {
        let frames = (lead * output_rate as f64).round().max(0.0) as usize;
        writer.write_silence(frames)?;
    }

    let mut bank = ChannelBank::new(channels, geom, options, 0, None);

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

    // The exact length is now known: the frames whose source position lies inside the
    // recording. The filter's tail — the frames the resampler can produce by
    // zero-padding past the end of the source — is not part of the material and is
    // dropped here rather than written.
    let limit = output_frames_for(decoder.frames_decoded(), geom);
    let tail = bank.flush();
    let tail_len = tail.first().map_or(0, |p| p.len());
    let wanted = limit.saturating_sub(source_frames).min(tail_len as u64) as usize;
    if wanted > 0 {
        let trimmed: Vec<Vec<f32>> = tail.iter().map(|plane| plane[..wanted].to_vec()).collect();
        writer.write_planar(&trimmed)?;
        source_frames += wanted as u64;
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
        resampled: bank.resampled,
    })
}

/// Render one source placed on the reference timeline.
///
/// Output frame `n` corresponds to reference time `n / output_rate`. Spans that the source
/// does not cover are written as digital silence and are explicitly reported as
/// placeholders.
///
/// The file has **exactly** the requested reference duration: not one frame more, not
/// one frame less, and with no filter tail past the end.
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
    ensure_wav_size_fits(total_frames, channels, options.format)?;

    // The output frames that carry source content.
    let content_start = ((model.offset) * output_rate as f64)
        .round()
        .clamp(0.0, total_frames as f64) as u64;
    let source_duration = meta.duration_secs;
    let content_end = (((model.offset + model.scale * source_duration) * output_rate as f64)
        .round()
        .clamp(0.0, total_frames as f64)) as u64;
    let content_end = content_end.max(content_start);
    let content_frames = content_end - content_start;

    let mut writer = WavWriter::create(out_path, output_rate, channels, options.format)?;

    // Leading silence placeholder.
    if content_start > 0 {
        writer.write_silence(content_start as usize)?;
    }

    // The resampler is told exactly how many frames it may produce, so the tail of the
    // filter can never be written past the end of the reference.
    let mut bank = ChannelBank::new(
        channels,
        geom,
        options,
        content_start as i64,
        Some(content_frames),
    );

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
        resampled: bank.resampled,
    })
}

/// A sequential cursor over one source, used by the merged timeline renderer.
///
/// The cursor keeps one processor per channel of the source, so a stereo source keeps
/// its two channels, and it is driven by that source's **own** sample rate.
struct SourceCursor {
    decoder: AudioDecoder,
    /// The position map this source's material is read through.
    geometry: Geometry,
    options: RenderOptions,
    /// The output frame the source's material starts at.
    start_frame: i64,
    /// One position map per channel of the source.
    processors: Vec<Processor>,
    /// Output frames not yet consumed, starting at frame index `pending_start`.
    pending: Vec<VecDeque<f32>>,
    /// The cursor's position on the timeline: the output frame index of the first
    /// frame still held in `pending`.
    ///
    /// This advances by exactly the frames that *leave* the cursor — the ones
    /// discarded because they lie before the requested span, and the ones handed to
    /// the caller. A cursor whose position did not advance would compute the next
    /// span's skip from a stale origin, throw away material it had already given
    /// away and read from further along the recording than the plan asked for,
    /// which is how a merged timeline ends up short and displaced.
    pending_start: i64,
    finished: bool,
    block_frames: usize,
}

impl SourceCursor {
    fn new(
        decoder: AudioDecoder,
        geom: Geometry,
        start_frame: i64,
        options: &RenderOptions,
    ) -> Self {
        Self {
            decoder,
            geometry: geom,
            options: options.clone(),
            start_frame,
            processors: Vec::new(),
            pending: Vec::new(),
            pending_start: start_frame,
            finished: false,
            block_frames: options.block_frames,
        }
    }

    /// Ensure output frames up to `target` are available in `pending`.
    fn fill_to(&mut self, target: i64) -> Result<()> {
        while self.pending_start + (self.pending_len() as i64) < target && !self.finished {
            let Some(block) = self.decoder.read_block(self.block_frames)? else {
                // End of stream: flush the processors and mark the cursor finished.
                for index in 0..self.processors.len() {
                    let tail = self.processors[index].flush();
                    self.pending[index].extend(tail);
                }
                self.finished = true;
                break;
            };
            if self.processors.is_empty() {
                // The channel count comes from the samples the decoder actually
                // hands over, not from the container's claim about it. A file whose
                // metadata names more channels than the stream carries would
                // otherwise leave a channel that never receives a sample, and a
                // frame only one channel has is not a frame of the recording.
                let channels = block.planes.len().max(1);
                self.processors = (0..channels)
                    .map(|_| Processor::new(self.geometry, &self.options, self.start_frame, None))
                    .collect();
                self.pending = vec![VecDeque::new(); channels];
            }
            for index in 0..self.processors.len() {
                let plane = block.planes.get(index).map(Vec::as_slice).unwrap_or(&[]);
                let out = self.processors[index].push(plane);
                self.pending[index].extend(out);
            }
        }
        Ok(())
    }

    /// The number of frames the channels hold **in common**.
    ///
    /// The channels are driven by one position map, so they hold the same frames.
    /// The minimum is taken so that a frame one channel does not have is never
    /// treated as a frame of the recording.
    fn pending_len(&self) -> usize {
        self.pending.iter().map(|p| p.len()).min().unwrap_or(0)
    }

    /// Take the frames in `[from, to)`, one plane per channel.
    ///
    /// Every channel returns the **same number of frames**: a frame that one channel
    /// does not have is not a frame of the recording, and the writer requires the
    /// planes it is given to be of equal length.
    fn take(&mut self, from: i64, to: i64) -> Result<Vec<Vec<f32>>> {
        self.fill_to(to)?;

        // The frames that lie before `from` are discarded.
        let skip = (from - self.pending_start).max(0) as usize;
        let mut discarded = 0usize;
        for pending in self.pending.iter_mut() {
            let removed = skip.min(pending.len());
            for _ in 0..removed {
                pending.pop_front();
            }
            discarded = discarded.max(removed);
        }
        self.pending_start += discarded as i64;

        // Only the frames every channel holds are frames of the recording.
        let want = (to - from).max(0) as usize;
        let count = want.min(self.pending_len());

        let mut out: Vec<Vec<f32>> = Vec::with_capacity(self.pending.len());
        for pending in self.pending.iter_mut() {
            let mut plane = Vec::with_capacity(count);
            for _ in 0..count {
                match pending.pop_front() {
                    Some(v) => plane.push(v),
                    None => break,
                }
            }
            out.push(plane);
        }

        // The frames that were handed over are gone, so the cursor now stands at the
        // frame after the last one it returned. A **short read** — the source ran
        // out before the span did — advances the cursor by what was actually
        // returned, not by what was asked for.
        self.pending_start += count as i64;
        Ok(out)
    }
}

/// Render every source onto one merged reference timeline.
///
/// The ownership of every span of the timeline comes from the report's timeline — the
/// same plan the GUI draws and the gap report describes — so the picture and the audio
/// cannot disagree.
///
/// Sources are resolved by priority (the order they were supplied). Overlapping material
/// is taken from the higher-priority source; the other source's material is not deleted
/// from its own per-source export.
pub fn render_merged_timeline(
    report: &AlignmentReport,
    out_path: &Path,
    options: &RenderOptions,
) -> Result<RenderResult> {
    anyhow::ensure!(!report.sources.is_empty(), "no sources to render");

    let source_paths: Vec<PathBuf> = report
        .sources
        .iter()
        .map(|a| a.source.path.clone())
        .collect();
    ensure_output_does_not_overwrite(out_path, &source_paths)?;

    // Open every source once. The decoder is kept so that the file is not opened —
    // and hashed — twice.
    let mut decoders: Vec<Option<AudioDecoder>> = Vec::with_capacity(report.sources.len());
    let mut source_channels: Vec<usize> = Vec::with_capacity(report.sources.len());
    let mut source_rates: Vec<u32> = Vec::with_capacity(report.sources.len());
    for alignment in &report.sources {
        match AudioDecoder::open(&alignment.source.path) {
            Ok(decoder) => {
                source_channels.push(decoder.meta().channels.max(1));
                source_rates.push(decoder.meta().sample_rate);
                decoders.push(Some(decoder));
            }
            Err(error) => {
                decoders.push(None);
                source_channels.push(alignment.source.channels.max(1));
                source_rates.push(alignment.source.sample_rate);
                let _ = error;
            }
        }
    }

    let max_channels = source_channels.iter().copied().max().unwrap_or(1).max(1);
    let out_channels = match options.channels {
        Some(c) if c < max_channels => {
            anyhow::bail!(
                "refusing to write a {}-channel merged timeline: one of the recordings has \
                 {} channels, and a narrower output would drop a channel of real audio. \
                 Export the recordings one at a time, or keep the {}-channel layout.",
                c,
                max_channels,
                max_channels
            );
        }
        Some(c) => c.max(1),
        None => max_channels,
    };

    let output_rate = options
        .sample_rate
        .unwrap_or(report.reference.sample_rate)
        .max(1);
    let total_frames = (report.timeline_duration * output_rate as f64)
        .round()
        .max(0.0) as u64;
    ensure_wav_size_fits(total_frames, out_channels, options.format)?;

    let mut writer = WavWriter::create(out_path, output_rate, out_channels, options.format)?;
    let mut cursors: Vec<Option<SourceCursor>> = (0..report.sources.len()).map(|_| None).collect();

    let mut cursor = 0u64;
    let mut source_frames = 0u64;
    let mut silence_frames = 0u64;
    // Whether any source's material was put through the resampler. The flag is
    // derived from the geometry that was actually used, so a merged timeline of
    // sources that all need no rate change reports `false` — the same rule the
    // two per-source layouts follow. Claiming otherwise would make the three
    // layouts disagree with each other about the same correction.
    let mut resampled = false;

    for span in &report.timeline {
        let start = (span.reference_start * output_rate as f64)
            .round()
            .clamp(0.0, total_frames as f64) as u64;
        let end = (span.reference_end * output_rate as f64)
            .round()
            .clamp(0.0, total_frames as f64) as u64;
        let end = end.max(start);
        if end <= cursor {
            continue;
        }
        // A span that begins before the cursor was already written.
        let start = start.max(cursor);
        let length = end - start;

        let owner = match span.source_index {
            Some(index)
                if matches!(span.kind, SegmentKind::Matched | SegmentKind::LowConfidence) =>
            {
                Some(index)
            }
            _ => None,
        };

        match owner {
            None => {
                writer.write_silence(length as usize)?;
                silence_frames += length;
                cursor = end;
            }
            Some(index) => {
                let alignment = &report.sources[index];
                let Some(model) = alignment.model() else {
                    // The source could not be aligned; it contributes nothing, and the
                    // span is left as silence rather than filled with another source's
                    // material at the wrong time.
                    writer.write_silence(length as usize)?;
                    silence_frames += length;
                    cursor = end;
                    continue;
                };
                let source_rate = source_rates[index];
                let geom = geometry_timeline(&model, source_rate, output_rate);
                resampled = resampled || !geom.is_unity_copy();
                if cursors[index].is_none() {
                    let decoder = decoders[index].take().ok_or_else(|| {
                        anyhow::anyhow!(
                            "the recording {} could not be opened for the merged timeline",
                            alignment.source.file_name
                        )
                    })?;
                    cursors[index] = Some(SourceCursor::new(decoder, geom, start as i64, options));
                }
                let cursor_ref = cursors[index].as_mut().expect("cursor just created");
                let frames = cursor_ref.take(start as i64, end as i64)?;
                let n = frames.first().map_or(0, |p| p.len());
                if n > 0 {
                    // The source's own channels go into the leading channels of the
                    // output. A source with fewer channels than the output leaves the
                    // remaining channels silent for its material: nothing is folded,
                    // nothing is duplicated, and nothing is lost.
                    let mut planes: Vec<Vec<f32>> = frames.clone();
                    while planes.len() < out_channels {
                        planes.push(vec![0.0f32; n]);
                    }
                    planes.truncate(out_channels);
                    writer.write_planar(&planes)?;
                    source_frames += n as u64;
                }
                let advanced = start + n as u64;
                if advanced < end {
                    writer.write_silence((end - advanced) as usize)?;
                    silence_frames += end - advanced;
                }
                cursor = end;
            }
        }
    }

    // Anything the plan did not describe is silence, and the plan is the authority.
    if cursor < total_frames {
        writer.write_silence((total_frames - cursor) as usize)?;
        silence_frames += total_frames - cursor;
    }

    let frames = writer.frames_written();
    writer.finalize()?;
    Ok(RenderResult {
        path: out_path.to_path_buf(),
        sample_rate: output_rate,
        channels: out_channels,
        frames,
        source_frames,
        silence_frames,
        // The merged timeline is the reference's own timeline: it is written at the
        // reference's rate, so its effective rate is one and it carries no pitch
        // shift of its own. Each source's drift correction is described by that
        // source's own model.
        effective_rate: 1.0,
        pitch_shift_cents: 0.0,
        resampled,
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
///
/// The comparison models the file system the export is going to, not the one the
/// code happens to run on: a name that differs only in case names the same file
/// on Windows and on a case-insensitive volume. See [`same_path`] for exactly
/// what is and is not folded.
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

/// Refuse an export whose planned outputs would land on one of the recordings.
///
/// Every file the export writes is checked, not only the corrected audio: the
/// export directory is often the directory the recordings are in, and a
/// recording called `syncmap.csv` would otherwise be replaced by the sidecar
/// of the same name.
pub fn ensure_outputs_avoid_inputs(outputs: &[PathBuf], inputs: &[PathBuf]) -> Result<()> {
    for output in outputs {
        ensure_output_does_not_overwrite(output, inputs)?;
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

/// Whether two paths name the same file on the file system the export targets.
///
/// The answer depends on the platform, because the file systems do:
///
/// * Windows compares names without regard to case.
/// * macOS and Linux normally do not, but a case-insensitive volume does.
///
/// Rather than guess, the comparison asks the operating system. An existing file
/// is identified by its canonical path, which resolves symbolic links and gives
/// the name the file system actually stores — so on a file system that
/// normalises names, two spellings of the same *existing* file are recognised
/// as the same file, because the canonical path is the stored spelling of both.
///
/// What is **not** done is folding Unicode normalisation into the comparison.
/// Two names that differ only in their Unicode normalisation are the same file
/// on a file system that normalises names, but the comparison does not know
/// that: it compares the names as they are written. The case where this could
/// matter — an output name that differs from a recording's name only in
/// normalisation, and the output does not exist yet — is therefore not
/// detected. Folding normalisation would need a Unicode database, and on
/// Windows it would be wrong: NTFS stores the name it is given, so two
/// differently-normalised names really are two files there.
fn same_path(a: &Path, b: &Path) -> bool {
    // An existing file may be reached through a link or through a different
    // spelling of its name; the canonical path settles that.
    if a.exists() && b.exists() {
        if let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            return folded(&ca.to_string_lossy()) == folded(&cb.to_string_lossy());
        }
    }
    match (comparable_path(a), comparable_path(b)) {
        (Some(pa), Some(pb)) => folded(&pa.to_string_lossy()) == folded(&pb.to_string_lossy()),
        _ => false,
    }
}

/// A path folded the way the platform folds names.
///
/// On Windows the path is compared without regard to case. Everywhere else it
/// is compared exactly, because a case-sensitive file system really does treat
/// `Song.wav` and `song.wav` as two files — and folding them anyway would
/// refuse an export that is perfectly safe.
fn folded(path: &str) -> String {
    #[cfg(windows)]
    {
        path.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
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

/// The corrected audio files an export writes for the given layout.
///
/// The names are worked out before anything is written, so that an export
/// which would land on one of the recordings can be refused before the first
/// file is touched. The *complete* set of files an export writes — this list
/// plus the sidecars — is [`complete_outputs`].
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

/// The complete set of files an export writes: the planned audio outputs and
/// the sidecars every export writes.
///
/// This is the whole manifest, not just the corrected audio: the sidecars —
/// the sync map, the segment table, the drift graph, the summary, the plan, the
/// report and the manifest — are files too, and a recording that happens to be
/// called `syncmap.csv` would be destroyed by one of them. Checking only the
/// audio left that path open.
///
/// The audio names are supplied by the caller rather than worked out here,
/// because the two front ends name their outputs differently on purpose: the
/// desktop sanitises a name that the operating system would refuse, and the
/// command line does not.
pub fn complete_outputs(out: &Path, planned: &[PathBuf]) -> Vec<PathBuf> {
    let mut outputs = planned.to_vec();
    outputs.extend(crate::export::sidecar_outputs(out));
    outputs
}
