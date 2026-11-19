//! Level-normalised RMS envelope — the primary alignment feature.
//!
//! The envelope is computed on a **logarithmic (dB)** scale and then **locally z-scored**.
//! Both steps matter for concert material:
//!
//! * dB scale — a 37 dB level difference between a soundboard and a watch recording becomes
//!   a constant offset, not a change of shape;
//! * local z-score — removes that constant offset *and* normalises the local dynamic range,
//!   so a loud passage and a quiet passage have comparable feature statistics.
//!
//! The resulting `norm` sequence is what the matcher correlates against.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::audio::AudioDecoder;

/// Configuration for envelope extraction.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeConfig {
    /// RMS window length in milliseconds.
    pub frame_ms: f64,
    /// Hop between successive envelope frames in milliseconds.
    pub hop_ms: f64,
    /// Half-width of the local z-score window in seconds.
    pub zscore_window_s: f64,
    /// Absolute clip applied to the z-scored values.
    pub zscore_clip: f32,
    /// dB above the estimated noise floor below which a frame counts as "quiet".
    pub quiet_margin_db: f32,
    /// Percentile of the frame-level distribution used as the noise-floor estimate.
    ///
    /// This has to be a *low* percentile. Music has no silence, so a percentile such as
    /// the 10th does not land on a noise floor — it lands on the quietest musical
    /// passage, which for compressed concert material sits only a few dB below the
    /// median. Using that as the floor then marks most of the recording as "quiet" and
    /// every fine probe is skipped. A 2nd-percentile estimate lands on genuine silence
    /// or on the recorder's own noise floor, which is what "quiet" is meant to mean.
    pub noise_floor_percentile: f64,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            frame_ms: 20.0,
            hop_ms: 10.0,
            zscore_window_s: 30.0,
            zscore_clip: 4.0,
            quiet_margin_db: 6.0,
            noise_floor_percentile: 0.02,
        }
    }
}

/// A computed RMS envelope.
#[derive(Debug, Clone)]
pub struct Envelope {
    /// Envelope frames per second.
    pub rate_hz: f64,
    /// Raw RMS level of each frame, in dBFS.
    pub db: Vec<f32>,
    /// Locally z-scored envelope (the feature used for correlation).
    pub norm: Vec<f32>,
    /// `true` where the frame is within `quiet_margin_db` of the estimated noise floor.
    pub quiet: Vec<bool>,
    /// Estimated noise floor of the file, in dBFS.
    pub noise_floor_db: f32,
    /// Level below which a frame is considered to be at the noise floor, in dBFS.
    pub quiet_threshold_db: f32,
    /// `true` when the noise-floor estimate had to fall back because the file has no
    /// quiet material at all (a constant-level recording).
    pub noise_floor_degenerate: bool,
    /// Duration covered by the envelope, in seconds.
    pub duration_secs: f64,
}

impl Envelope {
    /// Number of envelope frames.
    pub fn len(&self) -> usize {
        self.db.len()
    }

    /// `true` when the envelope holds no frames.
    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }

    /// Time (seconds) of envelope frame `i`.
    pub fn time_of(&self, i: usize) -> f64 {
        i as f64 / self.rate_hz
    }

    /// Index of the envelope frame nearest to `t` seconds.
    pub fn index_at(&self, t: f64) -> usize {
        ((t * self.rate_hz).round() as i64).clamp(0, self.len() as i64 - 1) as usize
    }

    /// Put features on a shared physical-time grid before correlation. Sample hops
    /// are rounded independently per file, so equal nominal hops are not equal clocks.
    pub fn at_rate(&self, rate_hz: f64) -> Self {
        let len = if self.is_empty() {
            0
        } else {
            (((self.len() - 1) as f64 * rate_hz / self.rate_hz).floor() as usize) + 1
        };
        let interpolate = |values: &[f32], position: f64| {
            let lo = (position.floor() as usize).min(values.len() - 1);
            let hi = (lo + 1).min(values.len() - 1);
            let f = (position - lo as f64) as f32;
            values[lo] * (1.0 - f) + values[hi] * f
        };
        let mut result = self.clone();
        result.rate_hz = rate_hz;
        result.db = (0..len)
            .map(|i| interpolate(&self.db, i as f64 * self.rate_hz / rate_hz))
            .collect();
        result.norm = (0..len)
            .map(|i| interpolate(&self.norm, i as f64 * self.rate_hz / rate_hz))
            .collect();
        result.quiet = (0..len)
            .map(|i| {
                let index = ((i as f64 * self.rate_hz / rate_hz).round() as usize)
                    .min(self.quiet.len() - 1);
                self.quiet[index]
            })
            .collect();
        result
    }

    /// Fraction of frames that are not marked quiet.
    pub fn active_fraction(&self) -> f64 {
        if self.quiet.is_empty() {
            return 0.0;
        }
        let active = self.quiet.iter().filter(|q| !**q).count();
        active as f64 / self.quiet.len() as f64
    }

    /// Mean of the normalised envelope over `[t0, t1]` seconds.
    pub fn mean_norm(&self, t0: f64, t1: f64) -> f64 {
        let lo = self.index_at(t0);
        let hi = self.index_at(t1).min(self.len().saturating_sub(1));
        if lo > hi {
            return 0.0;
        }
        let slice = &self.norm[lo..=hi];
        if slice.is_empty() {
            return 0.0;
        }
        slice.iter().map(|v| *v as f64).sum::<f64>() / slice.len() as f64
    }
}

/// Streaming envelope extractor: feed it decoded blocks, get an [`Envelope`] at the end.
pub struct EnvelopeExtractor {
    config: EnvelopeConfig,
    sample_rate: u32,
    frame_len: usize,
    hop_len: usize,
    ring: Vec<f32>,
    ring_idx: usize,
    sq_sum: f64,
    samples_seen: u64,
    db: Vec<f32>,
}

impl EnvelopeExtractor {
    /// Create an extractor for a given sample rate and configuration.
    pub fn new(sample_rate: u32, config: EnvelopeConfig) -> Self {
        let frame_len = ((config.frame_ms / 1000.0) * sample_rate as f64)
            .round()
            .max(1.0) as usize;
        let hop_len = ((config.hop_ms / 1000.0) * sample_rate as f64)
            .round()
            .max(1.0) as usize;
        Self {
            config,
            sample_rate,
            frame_len,
            hop_len,
            ring: vec![0.0; frame_len],
            ring_idx: 0,
            sq_sum: 0.0,
            samples_seen: 0,
            db: Vec::new(),
        }
    }

    /// Push a block of mono samples into the extractor.
    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            let old = self.ring[self.ring_idx];
            self.sq_sum += (s as f64) * (s as f64) - (old as f64) * (old as f64);
            self.ring[self.ring_idx] = s;
            self.ring_idx = (self.ring_idx + 1) % self.frame_len;
            self.samples_seen += 1;

            if self.samples_seen >= self.frame_len as u64
                && (self.samples_seen as usize - self.frame_len).is_multiple_of(self.hop_len)
            {
                let rms = (self.sq_sum / self.frame_len as f64).max(0.0).sqrt();
                self.db.push((20.0 * rms.max(1e-9).log10()) as f32);
            }
        }
    }

    /// Finalise the envelope: compute the noise floor, quiet flags and local z-score.
    pub fn finish(self) -> Envelope {
        let rate_hz = self.sample_rate as f64 / self.hop_len as f64;
        let db = self.db;

        // Noise floor: a low percentile of the frame levels, so that it lands on genuine
        // silence or on the recorder's own hiss rather than on a quiet musical passage.
        let mut noise_floor_db = percentile(&db, self.config.noise_floor_percentile);
        let mut quiet_threshold_db = noise_floor_db + self.config.quiet_margin_db;
        let mut quiet: Vec<bool> = db.iter().map(|v| *v < quiet_threshold_db).collect();

        // Degenerate case: the recording has no quiet material whatsoever (constant
        // level, or everything sits below the percentile estimate). Falling back to a
        // fixed range below the loudest frame keeps the matcher usable instead of
        // declaring the whole file silent.
        let quiet_fraction =
            quiet.iter().filter(|q| **q).count() as f64 / quiet.len().max(1) as f64;
        let noise_floor_degenerate = quiet_fraction > 0.95;
        if noise_floor_degenerate {
            let loudest = db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            noise_floor_db = loudest - 40.0;
            quiet_threshold_db = noise_floor_db + self.config.quiet_margin_db;
            quiet = db.iter().map(|v| *v < quiet_threshold_db).collect();
        }

        // The local statistics are taken from the frames that carry signal. A stretch of
        // silence inside the window would otherwise drag the mean down and inflate the
        // spread, which distorts the *shape* of the envelope around the silence and drags
        // the correlation peak away from the true correspondence.
        //
        // The shape is what the matcher correlates, and the correlation is invariant to
        // an affine change of either side, so what matters is that the *music* keeps
        // its own dynamic range. Taking the statistics from the signal frames does
        // exactly that; taking them from every frame would flatten the music wherever
        // the window holds a silence, and the reference's music — which has no silence
        // there — would not match it.
        //
        // The silent frames themselves are excluded from the correlation as well (see
        // `ncc_correlate_masked`), so they contribute to neither side of the comparison.
        let norm = local_zscore_masked(
            &db,
            (self.config.zscore_window_s * rate_hz).round().max(1.0) as usize,
            self.config.zscore_clip,
            &quiet,
        );

        let duration_secs = self.samples_seen as f64 / self.sample_rate as f64;

        Envelope {
            rate_hz,
            db,
            norm,
            quiet,
            noise_floor_db,
            quiet_threshold_db,
            noise_floor_degenerate,
            duration_secs,
        }
    }
}

/// Extract an envelope by streaming a whole file through the decoder.
///
/// When `cancel` is set, the extraction stops at the next block and returns an error.
pub fn extract_envelope(
    decoder: &mut AudioDecoder,
    config: EnvelopeConfig,
    block_frames: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Envelope> {
    let sample_rate = decoder.meta().sample_rate;
    let mut extractor = EnvelopeExtractor::new(sample_rate, config);
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

/// Percentile of a slice (linear interpolation between order statistics).
pub fn percentile(values: &[f32], p: f64) -> f32 {
    if values.is_empty() {
        return f32::NEG_INFINITY;
    }
    let mut v: Vec<f32> = values.to_vec();
    v.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (p.clamp(0.0, 1.0) * (v.len() - 1) as f64).round() as usize;
    v[idx]
}

/// Locally z-score a signal using a centred window of `half_window` frames.
///
/// Every frame is scored against the statistics of the whole window. See
/// [`local_zscore_masked`] for the variant that excludes silent frames from those
/// statistics.
pub fn local_zscore(values: &[f32], half_window: usize, clip: f32) -> Vec<f32> {
    local_zscore_masked(values, half_window, clip, &[])
}

/// The minimum number of signal frames a local window must hold before the local
/// statistics are taken from the signal frames alone.
const MIN_SIGNAL_FRAMES: usize = 8;

/// Locally z-score a signal, excluding silent frames from the local statistics.
///
/// The local mean and spread are computed over the frames that are *not* marked quiet.
/// This matters whenever a recording contains a stretch of silence: with the silence
/// included, the mean of the window is dragged down and the spread is inflated, so the
/// music around the silence is scored against the wrong statistics. The resulting shape
/// no longer matches the reference's shape, and the correlation peak moves — which shows
/// up as a correspondence that is wrong by seconds, or as no correspondence at all.
///
/// The reason is worth stating precisely, because the obvious alternative is wrong. The
/// matcher correlates the *shape* of the envelope, and the correlation is invariant to
/// an affine change of either side, so what matters is that the music keeps its own
/// dynamic range. Taking the statistics from the signal frames does that. Taking them
/// from every frame would flatten the music wherever the window holds a silence — and
/// the reference, which has no silence at that position, would not match it.
///
/// This was measured, not assumed: on a recording with a thirty-second recorder gap, the
/// all-frame statistics moved the measured correspondence by up to 6.5 seconds and left
/// every probe untrustworthy, while these statistics recovered it to within 0.1
/// milliseconds. The all-frame variant also makes the normalised shape depend on the
/// quiet mask, which is a dependence on where the silence is.
///
/// The silent frames themselves are still scored against the statistics of the signal
/// around them, so they read as strongly negative: silence is many standard deviations
/// below the local music, which is exactly what it is.
///
/// The matcher excludes those frames from the correlation itself, so the bias this
/// introduces into the *statistics* never reaches the measured correspondence.
///
/// If the window holds too few signal frames to give a stable estimate, the statistics
/// of the whole window are used instead.
pub fn local_zscore_masked(
    values: &[f32],
    half_window: usize,
    clip: f32,
    quiet: &[bool],
) -> Vec<f32> {
    let n = values.len();
    let mut out = vec![0.0f32; n];
    if n == 0 {
        return out;
    }

    // Prefix sums over the signal frames, and over all frames as a fallback.
    let mut pre_sum = vec![0.0f64; n + 1];
    let mut pre_sq = vec![0.0f64; n + 1];
    let mut pre_count = vec![0usize; n + 1];
    let mut pre_all_sum = vec![0.0f64; n + 1];
    let mut pre_all_sq = vec![0.0f64; n + 1];
    for i in 0..n {
        let v = values[i] as f64;
        pre_all_sum[i + 1] = pre_all_sum[i] + v;
        pre_all_sq[i + 1] = pre_all_sq[i] + v * v;
        let is_quiet = quiet.get(i).copied().unwrap_or(false);
        pre_count[i + 1] = pre_count[i] + usize::from(!is_quiet);
        if !is_quiet {
            pre_sum[i + 1] = pre_sum[i] + v;
            pre_sq[i + 1] = pre_sq[i] + v * v;
        } else {
            pre_sum[i + 1] = pre_sum[i];
            pre_sq[i + 1] = pre_sq[i];
        }
    }

    for i in 0..n {
        let lo = i.saturating_sub(half_window);
        let hi = (i + half_window).min(n - 1);
        let all_count = (hi - lo + 1) as f64;

        let signal_count = pre_count[hi + 1] - pre_count[lo];
        let (sum, sum_sq, count) = if signal_count >= MIN_SIGNAL_FRAMES {
            (
                pre_sum[hi + 1] - pre_sum[lo],
                pre_sq[hi + 1] - pre_sq[lo],
                signal_count as f64,
            )
        } else {
            (
                pre_all_sum[hi + 1] - pre_all_sum[lo],
                pre_all_sq[hi + 1] - pre_all_sq[lo],
                all_count,
            )
        };

        let mean = sum / count;
        let var = (sum_sq / count - mean * mean).max(0.0);
        let std = var.sqrt();
        let z = if std > 1e-6 {
            ((values[i] as f64 - mean) / std) as f32
        } else {
            0.0
        };
        out[i] = z.clamp(-clip, clip);
    }
    out
}
