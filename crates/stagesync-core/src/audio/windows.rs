//! Extraction of short windows of audio at known positions.
//!
//! The waveform-domain drift estimator needs the *raw* signal around a handful of
//! positions in a long recording. Decoding the file once per position would be
//! quadratic in the number of probes, so the whole file is decoded **once**, in one
//! streaming pass, and the requested windows are cut out of the stream as it goes.
//!
//! The memory cost is bounded by the *spacing* of the probes rather than by the length
//! of the recording: the buffer is trimmed as soon as the earliest pending window has
//! been completed. A three-hour recording with two dozen probes therefore holds a few
//! megabytes, not the whole file.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

use super::decode::AudioDecoder;
use crate::resample::{StreamingResampler, DEFAULT_PHASES, DEFAULT_TAPS};

/// Frames decoded per block while the stream is walked.
const BLOCK_FRAMES: usize = 1 << 15;

/// A window of audio to extract.
#[derive(Debug, Clone, Copy)]
pub struct WindowRequest {
    /// Centre of the window, in seconds from the start of the file.
    pub centre_secs: f64,
    /// Half the width of the window, in seconds.
    pub half_secs: f64,
}

impl WindowRequest {
    /// A window of the given total width, centred on `centre_secs`.
    pub fn centred(centre_secs: f64, width_secs: f64) -> Self {
        Self {
            centre_secs,
            half_secs: width_secs * 0.5,
        }
    }
}

/// The state of one window-extraction pass.
struct Extraction<'a> {
    /// The requests, sorted by position.
    plan: &'a [(usize, i64, i64)],
    /// The produced samples that have not been trimmed away.
    buffer: Vec<f32>,
    /// Index, in analysis-rate samples, of `buffer[0]`.
    buffer_start: i64,
    /// The next request in the plan that has not been resolved yet.
    next: usize,
    /// The result, one entry per request.
    out: Vec<Option<Vec<f32>>>,
}

impl Extraction<'_> {
    /// Whether every request has been resolved.
    fn done(&self) -> bool {
        self.next >= self.plan.len()
    }

    /// Take in newly produced samples and resolve the windows they complete.
    fn consume(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        while self.next < self.plan.len() {
            let (index, start, end) = self.plan[self.next];
            if start < 0 {
                // The window begins before the file does; there is nothing to take.
                self.out[index] = None;
                self.next += 1;
                continue;
            }
            if self.buffer_start + (self.buffer.len() as i64) < end {
                break;
            }
            if start < self.buffer_start {
                // The window's start has already been trimmed away.
                self.out[index] = None;
                self.next += 1;
                continue;
            }
            let lo = (start - self.buffer_start) as usize;
            let hi = ((end - self.buffer_start) as usize).min(self.buffer.len());
            if hi > lo {
                self.out[index] = Some(self.buffer[lo..hi].to_vec());
            }
            self.next += 1;
        }
        // Drop the samples that no pending window can need any more. When every
        // request has been resolved, that is all of them: the rest of the
        // recording is of no interest to this pass and must not be held.
        if self.done() {
            self.buffer.clear();
            self.buffer_start += self.buffer.len() as i64;
            return;
        }
        let keep_from = self.plan[self.next].1.max(0);
        if keep_from > self.buffer_start {
            let drop = ((keep_from - self.buffer_start) as usize).min(self.buffer.len());
            self.buffer.drain(..drop);
            self.buffer_start += drop as i64;
        }
    }
}

/// What one window-extraction pass cost.
///
/// The number of decoded frames is the honest measure of the work done: a pass
/// that is asked for one window near the beginning of a long recording should
/// not decode the whole recording to find it.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowStats {
    /// Frames decoded from the file.
    pub frames_decoded: u64,
}

/// Extract mono windows at the requested positions, in a single streaming pass.
///
/// The result has one entry per request, in the order the requests were given. An entry
/// is `None` when the window could not be produced: it starts before the file does, it
/// runs past the end of the file, or the file could not be decoded.
///
/// The samples are mono (the mean of the channels) and are resampled to `analysis_rate`
/// with the crate's deterministic polyphase resampler, so the position of every sample
/// on the timeline is known exactly.
///
/// The pass stops as soon as every request has been resolved. A source that is short
/// and sits near the beginning of a long reference therefore costs only the part of
/// the reference it actually occupies; the rest of it is never decoded and never held.
pub fn extract_windows(
    path: &Path,
    requests: &[WindowRequest],
    analysis_rate: u32,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Option<Vec<f32>>>> {
    extract_windows_with_stats(
        path,
        requests,
        analysis_rate,
        cancel,
        &mut WindowStats::default(),
    )
}

/// Extract windows, and report what the pass cost.
pub fn extract_windows_with_stats(
    path: &Path,
    requests: &[WindowRequest],
    analysis_rate: u32,
    cancel: Option<&AtomicBool>,
    stats: &mut WindowStats,
) -> Result<Vec<Option<Vec<f32>>>> {
    let out: Vec<Option<Vec<f32>>> = vec![None; requests.len()];
    if requests.is_empty() {
        return Ok(out);
    }
    if analysis_rate == 0 {
        anyhow::bail!("the analysis rate must be greater than zero");
    }

    let mut decoder = AudioDecoder::open(path)
        .with_context(|| format!("failed to open audio file: {}", path.display()))?;
    let sample_rate = decoder.meta().sample_rate;
    if sample_rate == 0 {
        anyhow::bail!("{} reports no sample rate", path.display());
    }
    let rate = analysis_rate as f64;
    let alpha = sample_rate as f64 / rate;

    // The plan is sorted by position so the buffer can be trimmed as the stream advances.
    let mut plan: Vec<(usize, i64, i64)> = requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            let start = ((request.centre_secs - request.half_secs) * rate).round() as i64;
            let end = ((request.centre_secs + request.half_secs) * rate).round() as i64;
            (index, start, end)
        })
        .collect();
    plan.sort_by_key(|(_, start, _)| *start);

    // The file's own length, when the container states one. A window that begins at
    // or after the end of the file cannot be produced, and saying so from the
    // container's own figure saves decoding the rest of the recording to find out.
    let duration_samples = if decoder.meta().duration_secs > 0.0 {
        (decoder.meta().duration_secs * rate).ceil() as i64
    } else {
        i64::MAX
    };

    let mut extraction = Extraction {
        plan: &plan,
        buffer: Vec::new(),
        buffer_start: 0,
        next: 0,
        out,
    };

    // Resolve the requests that cannot be satisfied before anything is decoded.
    while extraction.next < plan.len() {
        let (index, start, _) = plan[extraction.next];
        if start < 0 {
            extraction.out[index] = None;
            extraction.next += 1;
            continue;
        }
        if start >= duration_samples {
            extraction.out[index] = None;
            extraction.next += 1;
            continue;
        }
        break;
    }

    let mut resampler = StreamingResampler::new(alpha, 0.0, DEFAULT_TAPS, DEFAULT_PHASES);

    while !extraction.done() {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            anyhow::bail!("the analysis was cancelled");
        }
        let Some(block) = decoder.read_block(BLOCK_FRAMES)? else {
            break;
        };
        stats.frames_decoded = decoder.frames_decoded();
        let mono = block.mono();
        let produced = resampler.push(&mono);
        extraction.consume(&produced);
    }
    stats.frames_decoded = decoder.frames_decoded();
    let tail = resampler.flush();
    extraction.consume(&tail);

    Ok(extraction.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::wav::{WavFormat, WavWriter};

    /// Write a mono file whose sample value encodes its own position.
    fn ramp_file(path: &Path, rate: u32, seconds: f64) {
        let frames = (rate as f64 * seconds) as usize;
        let samples: Vec<f32> = (0..frames).map(|i| (i % 1000) as f32 / 1000.0).collect();
        let mut writer =
            WavWriter::create(path, rate, 1, WavFormat::Float32).expect("the file can be created");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("the samples fit");
        writer.finalize().expect("the file can be closed");
    }

    #[test]
    fn a_window_comes_back_at_the_requested_position() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8_000, 4.0);

        let windows = extract_windows(
            &path,
            &[
                WindowRequest::centred(1.0, 0.5),
                WindowRequest::centred(2.5, 0.5),
            ],
            8_000,
            None,
        )
        .expect("the windows can be extracted");

        assert_eq!(windows.len(), 2);
        for window in windows {
            let window = window.expect("the window is inside the file");
            assert_eq!(window.len(), 4_000);
        }
    }

    #[test]
    fn a_window_that_starts_before_the_file_is_not_invented() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8_000, 2.0);

        let windows = extract_windows(&path, &[WindowRequest::centred(0.1, 1.0)], 8_000, None)
            .expect("the windows can be extracted");

        assert!(
            windows[0].is_none(),
            "a window before the file was invented"
        );
    }

    #[test]
    fn a_window_that_runs_past_the_end_is_not_invented() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8_000, 2.0);

        let windows = extract_windows(&path, &[WindowRequest::centred(1.9, 1.0)], 8_000, None)
            .expect("the windows can be extracted");

        assert!(windows[0].is_none(), "a window past the end was invented");
    }

    #[test]
    fn windows_are_resampled_to_the_analysis_rate() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 48_000, 2.0);

        let windows = extract_windows(&path, &[WindowRequest::centred(1.0, 1.0)], 8_000, None)
            .expect("the windows can be extracted");

        let window = windows[0].as_ref().expect("the window is inside the file");
        assert_eq!(
            window.len(),
            8_000,
            "the window is not at the analysis rate"
        );
    }

    #[test]
    fn the_pass_stops_once_every_window_has_been_produced() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        // A minute of material, and a window at the very beginning of it.
        ramp_file(&path, 8_000, 60.0);

        let mut stats = WindowStats::default();
        let windows = extract_windows_with_stats(
            &path,
            &[WindowRequest::centred(0.5, 0.5)],
            8_000,
            None,
            &mut stats,
        )
        .expect("the windows can be extracted");

        assert!(
            windows[0].is_some(),
            "the window at the start of the file was not produced"
        );
        // The whole file is 480,000 frames. The pass must not decode all of it
        // to produce a window that ends one second in.
        assert!(
            stats.frames_decoded < 120_000,
            "the pass decoded {} frames of a 480,000-frame file to produce one window",
            stats.frames_decoded
        );
    }

    #[test]
    fn a_window_past_the_end_is_not_decoded_to_find() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8_000, 10.0);

        let mut stats = WindowStats::default();
        let windows = extract_windows_with_stats(
            &path,
            &[WindowRequest::centred(30.0, 1.0)],
            8_000,
            None,
            &mut stats,
        )
        .expect("the windows can be extracted");

        assert!(windows[0].is_none(), "a window past the end was invented");
        assert!(
            stats.frames_decoded == 0,
            "the pass decoded {} frames to find a window that is past the end of the file",
            stats.frames_decoded
        );
    }
}
