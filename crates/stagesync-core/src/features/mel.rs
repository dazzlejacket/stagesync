//! Log-mel spectrogram Ã¢â‚¬â€ the *independent* feature family.
//!
//! The RMS envelope is the primary matching feature. The log-mel spectrogram serves two
//! different purposes:
//!
//! 1. **Disambiguation** Ã¢â‚¬â€ envelope correlation of quasi-periodic music can produce false
//!    peaks; a spectral shape match is much less ambiguous for repeated sections.
//! 2. **Independent validation** Ã¢â‚¬â€ the holdout verifier uses spectral features while the
//!    model was fitted with envelopes, so a measurement is never simply confirming itself.
//!
//! Band edges are defined in **hertz**, so the feature is sample-rate independent: a 44.1 kHz
//! and a 48 kHz recording of the same performance produce comparable band sequences.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use rayon::prelude::*;

use crate::audio::AudioDecoder;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Configuration for log-mel extraction.
#[derive(Debug, Clone, Copy)]
pub struct LogMelConfig {
    /// Analysis window length in milliseconds.
    pub frame_ms: f64,
    /// Hop between successive frames in milliseconds.
    pub hop_ms: f64,
    /// Number of mel bands.
    pub n_bands: usize,
    /// Lowest band edge in hertz.
    pub f_min_hz: f64,
    /// Highest band edge in hertz (clamped to below Nyquist).
    pub f_max_hz: f64,
    /// Subtract each frame's mean band value (removes the overall level of the frame).
    pub normalise_frames: bool,
}

impl Default for LogMelConfig {
    fn default() -> Self {
        Self {
            frame_ms: 64.0,
            hop_ms: 32.0,
            n_bands: 32,
            f_min_hz: 40.0,
            f_max_hz: 8000.0,
            normalise_frames: true,
        }
    }
}

/// A computed log-mel spectrogram, stored band-major.
#[derive(Debug, Clone)]
pub struct LogMel {
    /// Frames per second.
    pub rate_hz: f64,
    /// Number of mel bands.
    pub n_bands: usize,
    /// Number of time frames.
    pub n_frames: usize,
    /// Band-major matrix: `values[band * n_frames + frame]`.
    pub values: Vec<f32>,
}

impl LogMel {
    /// One band as a time series.
    pub fn band(&self, band: usize) -> &[f32] {
        &self.values[band * self.n_frames..(band + 1) * self.n_frames]
    }

    /// Time (seconds) of frame `i`.
    pub fn time_of(&self, i: usize) -> f64 {
        i as f64 / self.rate_hz
    }

    /// Index of the frame nearest to `t` seconds.
    pub fn index_at(&self, t: f64) -> usize {
        ((t * self.rate_hz).round() as i64).clamp(0, self.n_frames as i64 - 1) as usize
    }

    /// Extract a contiguous band-major sub-matrix covering `[t0, t1]` seconds.
    pub fn window(&self, t0: f64, t1: f64) -> LogMel {
        let lo = self.index_at(t0);
        let hi = self.index_at(t1).min(self.n_frames.saturating_sub(1));
        let n = hi.saturating_sub(lo) + 1;
        let mut values = Vec::with_capacity(self.n_bands * n);
        for b in 0..self.n_bands {
            let start = b * self.n_frames + lo;
            values.extend_from_slice(&self.values[start..start + n]);
        }
        LogMel {
            rate_hz: self.rate_hz,
            n_bands: self.n_bands,
            n_frames: n,
            values,
        }
    }
}

/// Streaming log-mel extractor.
pub struct LogMelExtractor {
    config: LogMelConfig,
    frame_len: usize,
    hop_len: usize,
    fft_size: usize,
    fft: Arc<dyn Fft<f32>>,
    filterbank: Vec<(usize, Vec<f32>)>,
    window: Vec<f32>,
    ring: Vec<f32>,
    ring_idx: usize,
    samples_seen: u64,
    /// Frames waiting to be transformed.
    batch: Vec<Vec<f32>>,
    out_bands: Vec<Vec<f32>>,
    n_out_frames: usize,
}

impl LogMelExtractor {
    /// Create an extractor for a given sample rate.
    pub fn new(sample_rate: u32, config: LogMelConfig) -> Self {
        let frame_len = ((config.frame_ms / 1000.0) * sample_rate as f64)
            .round()
            .max(1.0) as usize;
        let hop_len = ((config.hop_ms / 1000.0) * sample_rate as f64)
            .round()
            .max(1.0) as usize;
        let fft_size = frame_len.next_power_of_two().max(64);

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let nyquist = sample_rate as f64 / 2.0;
        let f_max = config.f_max_hz.min(nyquist * 0.95);
        let filterbank = build_filterbank(
            config.n_bands,
            config.f_min_hz,
            f_max,
            sample_rate as f64,
            fft_size,
        );

        let window: Vec<f32> = (0..frame_len)
            .map(|i| {
                let hann =
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / frame_len as f64).cos();
                hann as f32
            })
            .collect();

        let out_bands = vec![Vec::new(); config.n_bands];

        Self {
            config,
            frame_len,
            hop_len,
            fft_size,
            fft,
            filterbank,
            window,
            ring: vec![0.0; frame_len],
            ring_idx: 0,
            samples_seen: 0,
            batch: Vec::new(),
            out_bands,
            n_out_frames: 0,
        }
    }

    /// Push a block of mono samples into the extractor.
    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.ring[self.ring_idx] = s;
            self.ring_idx = (self.ring_idx + 1) % self.frame_len;
            self.samples_seen += 1;

            if self.samples_seen >= self.frame_len as u64
                && (self.samples_seen as usize - self.frame_len) % self.hop_len == 0
            {
                let frame: Vec<f32> = self
                    .ring
                    .iter()
                    .copied()
                    .chain(std::iter::repeat(0.0))
                    .take(self.frame_len)
                    .collect();
                self.batch.push(frame);
                if self.batch.len() >= 512 {
                    self.flush_batch();
                }
            }
        }
    }

    fn flush_batch(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let fft = Arc::clone(&self.fft);
        let window = &self.window;
        let filterbank = &self.filterbank;
        let fft_size = self.fft_size;
        let normalise = self.config.normalise_frames;

        // One spectrum per frame, computed in parallel.
        let spectra: Vec<Vec<f32>> = self
            .batch
            .par_iter()
            .map(|frame| {
                let mut buf: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); fft_size];
                for i in 0..frame.len().min(fft_size) {
                    buf[i] = Complex32::new(frame[i] * window[i], 0.0);
                }
                fft.process(&mut buf);
                let bins = fft_size / 2 + 1;
                let mut power = vec![0.0f32; bins];
                for (i, p) in power.iter_mut().enumerate() {
                    let c = buf[i];
                    *p = c.norm_sqr();
                }
                power
            })
            .collect();

        for power in &spectra {
            let mut bands = vec![0.0f32; filterbank.len()];
            for (b, (start, weights)) in filterbank.iter().enumerate() {
                let mut acc = 0.0f32;
                for (k, w) in weights.iter().enumerate() {
                    acc += power[start + k] * w;
                }
                bands[b] = (acc.max(1e-10)).ln();
            }
            if normalise {
                let mean = bands.iter().sum::<f32>() / bands.len() as f32;
                for v in bands.iter_mut() {
                    *v -= mean;
                }
            }
            for (b, v) in bands.iter().enumerate() {
                self.out_bands[b].push(*v);
            }
            self.n_out_frames += 1;
        }
        self.batch.clear();
    }

    /// Finalise the spectrogram.
    pub fn finish(mut self) -> LogMel {
        self.flush_batch();
        let mut values = Vec::with_capacity(self.out_bands.len() * self.n_out_frames);
        for band in &self.out_bands {
            values.extend_from_slice(band);
        }
        LogMel {
            rate_hz: 1000.0 / self.config.hop_ms,
            n_bands: self.config.n_bands,
            n_frames: self.n_out_frames,
            values,
        }
    }
}

/// Extract a log-mel spectrogram by streaming a whole file through the decoder.
///
/// When `cancel` is set, the extraction stops at the next block and returns an error.
pub fn extract_log_mel(
    decoder: &mut AudioDecoder,
    config: LogMelConfig,
    block_frames: usize,
    cancel: Option<&AtomicBool>,
) -> Result<LogMel> {
    let sample_rate = decoder.meta().sample_rate;
    let mut extractor = LogMelExtractor::new(sample_rate, config);
    decoder.for_each_block(block_frames, |block| {
        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                anyhow::bail!("the analysis was cancelled");
            }
        }
        extractor.push(&block.mono());
        Ok(())
    })?;
    Ok(extractor.finish())
}

/// Convert a frequency in hertz to the mel scale.
pub fn hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Convert a value on the mel scale back to hertz.
pub fn mel_to_hz(mel: f64) -> f64 {
    700.0 * (10.0f64.powf(mel / 2595.0) - 1.0)
}

/// Build a sparse triangular mel filterbank over the FFT bin frequencies.
fn build_filterbank(
    n_bands: usize,
    f_min: f64,
    f_max: f64,
    sample_rate: f64,
    fft_size: usize,
) -> Vec<(usize, Vec<f32>)> {
    let bins = fft_size / 2 + 1;
    let bin_hz = sample_rate / fft_size as f64;
    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);
    let edges: Vec<f64> = (0..n_bands + 2)
        .map(|i| {
            let m = mel_min + (mel_max - mel_min) * i as f64 / (n_bands + 1) as f64;
            mel_to_hz(m)
        })
        .collect();

    let mut filterbank = Vec::with_capacity(n_bands);
    for b in 0..n_bands {
        let (lo, mid, hi) = (edges[b], edges[b + 1], edges[b + 2]);
        let lo_bin = ((lo / bin_hz).floor() as isize).max(0) as usize;
        let hi_bin = (((hi / bin_hz).ceil() as isize).max(0) as usize).min(bins - 1);
        let mut weights = Vec::new();
        let mut start = lo_bin;
        for bin in lo_bin..=hi_bin {
            let f = bin as f64 * bin_hz;
            let w = if f <= mid && mid > lo {
                (f - lo) / (mid - lo)
            } else if f > mid && hi > mid {
                (hi - f) / (hi - mid)
            } else {
                0.0
            };
            if w > 0.0 {
                if weights.is_empty() {
                    start = bin;
                }
                weights.push(w as f32);
            }
        }
        if weights.is_empty() {
            weights.push(0.0);
        }
        filterbank.push((start, weights));
    }
    filterbank
}
