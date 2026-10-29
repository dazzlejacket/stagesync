//! Independent holdout verification.
//!
//! The alignment model is fitted with RMS-envelope correlation. If the *same* estimator
//! is then used to check its own answer, the check proves nothing: a biased estimator
//! would be reported as a confident one.
//!
//! This module therefore verifies the **rendered** result with a different method:
//!
//! * the material is measured on the **raw waveform** with GCC-PHAT (phase-based delay
//!   estimation), not on the envelope;
//! * the measurement windows are **disjoint** from the windows used to fit the model —
//!   they sit between the fine-map probes, so they are genuine hold-out positions;
//! * the residual statistics are reported as p50 / p95 / max, so a single bad position
//!   cannot hide behind an average;
//! * the pitch change introduced by the resampling is measured independently.
//!
//! The sign convention is: **a positive residual means the corrected audio is ahead of
//! the reference**.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::WavReader;
use crate::matching::dsp::gcc_phat;
use crate::matching::model::LinearModel;
use crate::resample::StreamingResampler;

/// Configuration for the holdout verification.
#[derive(Debug, Clone, Copy)]
pub struct VerifyConfig {
    /// Rate at which the waveform comparison is performed, in Hz.
    pub analysis_rate: u32,
    /// Length of each verification window, in seconds.
    pub window_secs: f64,
    /// Maximum lag searched for, in seconds.
    pub max_lag_secs: f64,
    /// Spacing of the fine-map probes, in seconds (used to place the hold-out windows).
    pub fine_hop_secs: f64,
    /// Minimum peak separation required for a point to count towards the statistics.
    pub min_zscore: f32,
    /// Minimum correlation required for a point to count towards the statistics.
    pub min_peak: f32,
    /// The pitch change the applied model implies, in cents.
    ///
    /// The measured value is compared against it and the difference reported. The
    /// measurement is a coarse sanity check only: an autocorrelation pitch tracker on
    /// polyphonic material resolves to tens of cents, which is far coarser than the
    /// fraction of a cent that a ppm-scale drift correction produces.
    pub expected_pitch_cents: Option<f64>,
    /// How far the two hold-out methods have to agree before a position is trusted.
    ///
    /// A position where the waveform method and the envelope method disagree by more
    /// than this is *disputed*: at least one of them has locked onto the wrong thing,
    /// and the position is excluded from the headline statistics rather than being
    /// quietly averaged in.
    pub disagree_ms: f64,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            analysis_rate: 16_000,
            window_secs: 20.0,
            max_lag_secs: 0.5,
            fine_hop_secs: 15.0,
            min_zscore: 4.0,
            min_peak: 0.05,
            expected_pitch_cents: None,
            disagree_ms: 10.0,
        }
    }
}

/// One hold-out measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutPoint {
    /// Position in the source, in seconds.
    ///
    /// The residuals are measured against the model, so the position in the source is
    /// what turns a residual into a statement about the drift.
    pub source_time: f64,
    /// Reference time of the measurement, in seconds.
    pub reference_time: f64,
    /// Measured residual in milliseconds. Positive means the corrected audio is ahead.
    pub residual_ms: f64,
    /// Peak value of the GCC-PHAT correlation.
    pub phat_peak: f32,
    /// How many standard deviations the peak stood above the sidelobes.
    pub phat_zscore: f32,
    /// `true` when the point was good enough to count towards the statistics.
    pub counted: bool,
    /// Residual measured with the RMS-envelope method, in milliseconds.
    ///
    /// `None` when the envelope measurement could not be made. This second opinion
    /// works across genuinely different recordings of the same performance, where the
    /// waveform method cannot.
    pub envelope_residual_ms: Option<f64>,
    /// Normalised correlation of the envelope peak.
    pub envelope_peak: Option<f32>,
}

/// The outcome of the holdout verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutResult {
    /// Every measurement.
    pub points: Vec<HoldoutPoint>,
    /// Median of the counted residuals, in milliseconds.
    pub p50_ms: f64,
    /// 95th percentile of the counted residuals, in milliseconds.
    pub p95_ms: f64,
    /// Largest absolute residual, in milliseconds.
    pub max_ms: f64,
    /// Number of points that counted towards the statistics.
    pub counted: usize,
    /// Residual statistics over every counted point, including the disputed ones.
    ///
    /// The headline figures (`p50_ms`, `p95_ms`, `max_ms`) exclude the disputed
    /// positions; these are the unfiltered numbers.
    pub raw_counted: usize,
    pub raw_p50_ms: f64,
    pub raw_p95_ms: f64,
    pub raw_max_ms: f64,
    /// Number of positions where the two hold-out methods disagreed.
    pub disputed: usize,
    /// Residual statistics from the RMS-envelope method, in milliseconds.
    pub envelope_p50_ms: f64,
    pub envelope_p95_ms: f64,
    pub envelope_max_ms: f64,
    /// Number of points the envelope method could measure.
    pub envelope_counted: usize,
    /// Number of envelope measurements discarded as outliers.
    pub envelope_rejected: usize,
    /// Measured pitch change of the corrected material, in cents (negative = down).
    pub pitch_cents: f64,
    /// Rate used for the waveform comparison.
    pub analysis_rate: u32,
    /// How wrong the applied drift is, in parts per million, according to the hold-out.
    ///
    /// A correctly corrected file has *flat* residuals: the same offset error
    /// everywhere. A slope in the residuals against the source time means the applied
    /// drift is wrong, and the slope says by how much. This is measured on the
    /// **rendered** file, so it is a statement about what was actually applied rather
    /// than a restatement of the model that was fitted.
    ///
    /// `None` when there were too few mutually corroborated positions to fit a slope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_error_ppm: Option<f64>,
    /// The standard error of [`HoldoutResult::drift_error_ppm`], in parts per million.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_error_se_ppm: Option<f64>,
    /// Notes about the verification.
    pub notes: Vec<String>,
}

/// Verify a rendered, corrected source against the reference.
///
/// `corrected_path` must be the **source-aligned** render: its frame `n` corresponds to
/// reference time `model.offset + n / sample_rate`.
pub fn verify_corrected(
    corrected_path: &Path,
    reference_path: &Path,
    model: &LinearModel,
    source_duration: f64,
    config: &VerifyConfig,
) -> Result<HoldoutResult> {
    let mut notes = Vec::new();

    let mut corrected = WavReader::open(corrected_path).with_context(|| {
        format!(
            "failed to open corrected file: {}",
            corrected_path.display()
        )
    })?;
    let mut reference = WavReader::open(reference_path).with_context(|| {
        format!(
            "failed to open reference file: {}",
            reference_path.display()
        )
    })?;

    let corrected_rate = corrected.sample_rate();
    let reference_rate = reference.sample_rate();
    let window = (config.window_secs * config.analysis_rate as f64).round() as usize;
    let margin = (config.analysis_rate as f64 * 0.05) as usize; // 50 ms of context
    let max_lag = (config.max_lag_secs * config.analysis_rate as f64).round() as usize;

    let positions = choose_holdout_positions(source_duration, config);
    if positions.len() < 3 {
        notes.push(format!(
            "only {} hold-out positions were available; at least 3 are needed for a meaningful check",
            positions.len()
        ));
    }

    let mut points: Vec<HoldoutPoint> = Vec::new();
    let mut pitch_ratios: Vec<f64> = Vec::new();

    for &u in &positions {
        // Reference time of the position, and the frame index in each file.
        let reference_time = model.offset + u;
        if reference_time < 0.0 {
            continue;
        }
        let corrected_frame = (u * corrected_rate as f64).round().max(0.0) as u64;
        let reference_frame = (reference_time * reference_rate as f64).round().max(0.0) as u64;

        let corr_sig = read_resampled(
            &mut corrected,
            corrected_frame,
            window,
            margin,
            corrected_rate,
            config.analysis_rate,
        )?;
        let ref_sig = read_resampled(
            &mut reference,
            reference_frame,
            window,
            margin,
            reference_rate,
            config.analysis_rate,
        )?;

        if corr_sig.len() < window || ref_sig.len() < window {
            continue;
        }

        // --- Opinion 1: the raw waveform, phase-based delay estimation ------------
        // reference against corrected: a positive lag means the corrected audio is ahead.
        let phat = gcc_phat(&ref_sig, &corr_sig, max_lag);
        let (residual_ms, phat_peak, phat_zscore, counted) = match phat {
            Some(p) => {
                let counted = p.zscore >= config.min_zscore && p.peak.abs() >= config.min_peak;
                (
                    p.lag / config.analysis_rate as f64 * 1000.0,
                    p.peak,
                    p.zscore,
                    counted,
                )
            }
            None => (0.0, 0.0, 0.0, false),
        };

        // --- Opinion 2: the RMS envelope ----------------------------------------
        // The waveform method only has meaning when the two recordings are the same
        // signal. On real material they are different recordings of the same
        // performance, and the envelope comparison is the usable evidence.
        let corr_env = rms_envelope(&corr_sig, config.analysis_rate);
        let ref_env = rms_envelope(&ref_sig, config.analysis_rate);
        let env_max_lag = (config.max_lag_secs * ENVELOPE_RATE).round() as usize;
        let env = envelope_offset(&corr_env, &ref_env, env_max_lag);
        let (envelope_residual_ms, envelope_peak) = match env {
            Some((lag, peak)) => (Some(lag / ENVELOPE_RATE * 1000.0), Some(peak)),
            None => (None, None),
        };

        points.push(HoldoutPoint {
            source_time: u,
            reference_time,
            residual_ms,
            phat_peak,
            phat_zscore,
            counted,
            envelope_residual_ms,
            envelope_peak,
        });

        // Independent pitch check at the same position.
        //
        // The reference's fundamental is found freely; the corrected signal's is then
        // searched for *near* that value. Polyphonic material has many autocorrelation
        // peaks, and letting each signal pick its own global maximum can put them on
        // different harmonics — which would make the measured pitch change meaningless.
        // A drift correction of a few ppm cannot move a partial by more than a
        // hundredth of a percent, so a tight band around the reference's own period is
        // the only comparison that means anything.
        if let Some(f_ref) = estimate_f0(&ref_sig, config.analysis_rate, 70.0, 800.0) {
            if let Some(f_corr) = estimate_f0_near(&corr_sig, config.analysis_rate, f_ref, 0.03) {
                if f_corr > 0.0 && f_ref > 0.0 {
                    pitch_ratios.push(f_corr / f_ref);
                }
            }
        }
    }

    let counted: Vec<f64> = points
        .iter()
        .filter(|p| p.counted)
        .map(|p| p.residual_ms)
        .collect();

    // A position where the two methods disagree is not evidence of anything: at least
    // one of them locked onto the wrong thing. Those positions are excluded from the
    // headline statistics and counted separately.
    let disputed: Vec<bool> = points
        .iter()
        .map(|p| match p.envelope_residual_ms {
            Some(env) => (env - p.residual_ms).abs() > config.disagree_ms,
            None => false,
        })
        .collect();
    let disputed_count = disputed.iter().filter(|d| **d).count();

    let agreed: Vec<f64> = points
        .iter()
        .zip(disputed.iter())
        .filter(|(p, d)| p.counted && !**d)
        .map(|(p, _)| p.residual_ms)
        .collect();
    let (p50, p95, max) = residual_stats(&agreed);
    let (raw_p50, raw_p95, raw_max) = residual_stats(&counted);

    // How wrong the applied drift is, according to the rendered file.
    //
    // The residuals of a correctly corrected file are flat: the same offset error
    // everywhere. A slope in them against the source time means the applied *rate* is
    // wrong, and the slope says by how much. Because this is measured on the rendered
    // audio, it is a statement about what was actually applied — not a restatement of
    // the model that was fitted.
    let agreed_with_time: Vec<(f64, f64)> = points
        .iter()
        .zip(disputed.iter())
        .filter(|(p, d)| p.counted && !**d)
        .map(|(p, _)| (p.source_time, p.residual_ms))
        .collect();
    let (drift_error_ppm, drift_error_se_ppm) = drift_error(&agreed_with_time);
    if let (Some(error), Some(error_se)) = (drift_error_ppm, drift_error_se_ppm) {
        notes.push(format!(
            "the hold-out residuals change by {:+.3} ppm across the source, so the drift that was \
             applied is out by {:+.3} ppm ± {:.3} ppm; a drift that is out by 20 ppm would leave a \
             residual of about 100 ms at the end of a file this long",
            -error, error, error_se
        ));
    } else if agreed.len() >= 3 {
        notes.push(
            "the hold-out residuals were too few or too scattered to give a slope, so no independent \
             figure for the drift error could be established"
                .to_string(),
        );
    }

    // The largest residual is worth naming, because a single weak correlation can be the
    // whole of the "max" figure. Saying which position it was, how strong the
    // correlation was there, and what the other method said is more useful than the
    // number on its own.
    if let Some(worst) = points
        .iter()
        .zip(disputed.iter())
        .filter(|(p, d)| p.counted && !**d)
        .max_by(|a, b| a.0.residual_ms.abs().total_cmp(&b.0.residual_ms.abs()))
        .map(|(p, _)| p)
    {
        let mut peaks: Vec<f32> = points
            .iter()
            .zip(disputed.iter())
            .filter(|(p, d)| p.counted && !**d)
            .map(|(p, _)| p.phat_peak)
            .collect();
        peaks.sort_by(|a, b| a.total_cmp(b));
        let median_peak = if peaks.is_empty() {
            0.0
        } else {
            peaks[peaks.len() / 2]
        };
        let other = match worst.envelope_residual_ms {
            Some(value) => {
                format!("the RMS-envelope method measured {value:+.3} ms at the same position")
            }
            None => "the RMS-envelope method could not measure that position".to_string(),
        };
        notes.push(format!(
            "the largest residual ({:+.3} ms) is at {:.1} s, where the waveform correlation peak was \
             only {:.3} against a median of {:.3}; {other}",
            worst.residual_ms, worst.source_time, worst.phat_peak, median_peak
        ));
    }

    let env_all: Vec<f64> = points
        .iter()
        .filter_map(|p| p.envelope_residual_ms)
        .collect();
    let (env_kept, env_rejected) = reject_outliers(&env_all);
    let (env_p50, env_p95, env_max) = residual_stats(&env_kept);
    if env_rejected > 0 {
        notes.push(format!(
            "{env_rejected} of {} RMS-envelope hold-out measurements were discarded as outliers; a quiet \
             passage can match an unrelated moment of the reference, and those positions land at the very \
             edge of the search window",
            env_all.len()
        ));
    }

    if counted.is_empty() && env_kept.is_empty() {
        notes.push(
            "no hold-out position produced a usable measurement; the correction could not be \
             verified independently"
                .to_string(),
        );
    } else if counted.is_empty() {
        notes.push(
            "the waveform cross-correlation produced no trustworthy peak, which is expected \
             when the corrected audio and the reference are different recordings of the same \
             performance; the RMS-envelope hold-out check is the usable evidence"
                .to_string(),
        );
    }

    let pitch_cents = if pitch_ratios.is_empty() {
        0.0
    } else {
        let mean: f64 = pitch_ratios.iter().sum::<f64>() / pitch_ratios.len() as f64;
        1200.0 * mean.log2()
    };

    if let Some(expected) = config.expected_pitch_cents {
        let difference = pitch_cents - expected;
        if difference.abs() > 1.0 {
            notes.push(format!(
                "the measured pitch change of {pitch_cents:+.4} cents differs from the {expected:+.4} cents \
                 the model implies by {difference:+.4} cents; an autocorrelation pitch tracker on polyphonic \
                 material resolves to tens of cents, so the measured value is a coarse sanity check rather \
                 than a measurement of the correction"
            ));
        }
    }

    if disputed_count > 0 {
        notes.push(format!(
            "{disputed_count} of {} hold-out positions are disputed: the waveform method and the RMS-envelope \
             method disagreed by more than {:.0} ms, so at least one of them locked onto the wrong thing. \
             Those positions are excluded from the headline statistics.",
            points.len(),
            config.disagree_ms
        ));
    }
    if agreed.len() < 5 {
        notes.push(format!(
            "only {} hold-out positions were mutually corroborated; fewer than 5, so no accuracy claim is \
             established by this run",
            agreed.len()
        ));
    }

    Ok(HoldoutResult {
        points,
        p50_ms: p50,
        p95_ms: p95,
        max_ms: max,
        counted: agreed.len(),
        raw_p50_ms: raw_p50,
        raw_p95_ms: raw_p95,
        raw_max_ms: raw_max,
        raw_counted: counted.len(),
        disputed: disputed_count,
        envelope_p50_ms: env_p50,
        envelope_p95_ms: env_p95,
        envelope_max_ms: env_max,
        envelope_counted: env_kept.len(),
        envelope_rejected: env_rejected,
        pitch_cents,
        analysis_rate: config.analysis_rate,
        drift_error_ppm,
        drift_error_se_ppm,
        notes,
    })
}

/// The drift error implied by the hold-out residuals, in parts per million.
///
/// The residuals are in milliseconds and the source times in seconds, so the slope of
/// the fit is in milliseconds per second. One part per million of drift shows up as
/// one microsecond per second, that is `0.001` milliseconds per second, so the slope
/// is multiplied by a thousand.
///
/// The sign: the residual is positive when the corrected audio is *ahead* of the
/// reference. A drift that is too small leaves the corrected audio progressively
/// ahead of where it should be, so the slope is the negative of the error.
fn drift_error(points: &[(f64, f64)]) -> (Option<f64>, Option<f64>) {
    let Some(fit) = crate::matching::drift::fit_with_uncertainty(points) else {
        return (None, None);
    };
    if !fit.slope_se.is_finite() {
        return (None, None);
    }
    (Some(-fit.slope * 1000.0), Some(fit.slope_se * 1000.0))
}

/// Choose hold-out positions that sit *between* the fine-map probes.
pub fn choose_holdout_positions(source_duration: f64, config: &VerifyConfig) -> Vec<f64> {
    let step = (config.fine_hop_secs * 2.0).max(1.0);
    let offset = config.fine_hop_secs / 2.0;
    let mut out = Vec::new();
    let mut u = offset;
    while u < source_duration - config.window_secs - config.fine_hop_secs {
        out.push(u);
        u += step;
    }
    out
}

/// Read a window and resample it to the analysis rate, trimming the context margins.
///
/// `start_frame` is in **source** frames. `frames` and `margin` are in **output**
/// (analysis-rate) samples.
fn read_resampled(
    reader: &mut WavReader,
    start_frame: u64,
    frames: usize,
    margin: usize,
    src_rate: u32,
    dst_rate: u32,
) -> Result<Vec<f32>> {
    // The resampler's `alpha` is the number of *input* frames advanced per output
    // frame, i.e. `fs_in / fs_out`. Getting this the wrong way round both inverts the
    // anti-aliasing cutoff and produces the wrong number of output samples.
    let ratio = src_rate as f64 / dst_rate as f64;
    let margin_in = (margin as f64 * ratio).ceil() as usize;
    let need_in = margin_in * 2 + (frames as f64 * ratio).ceil() as usize;
    let start = start_frame.saturating_sub(margin_in as u64);
    let raw = reader.read_mono(start, need_in)?;

    let mut resampler = StreamingResampler::new(ratio, 0.0, 63, 1024);
    let mut out = resampler.push(&raw);
    out.extend(resampler.flush());

    // Output sample `m` is taken from input position `ratio * m`, so the window of
    // interest begins exactly at output index `margin`.
    let start_out = margin;
    if start_out >= out.len() {
        return Ok(Vec::new());
    }
    let end = (start_out + frames).min(out.len());
    Ok(out[start_out..end].to_vec())
}

/// The RMS envelope of a signal, at 100 frames per second.
///
/// This is the *second* hold-out opinion. The RMS envelope is what the alignment model
/// was fitted with, so an envelope measurement is not independent of the fitter — but
/// the waveform GCC-PHAT measurement is only meaningful when the two recordings are
/// actually the same signal. A soundboard mixdown and a body microphone are different
/// recordings of the same performance, so the envelope check is often the only usable
/// evidence on real material.
fn rms_envelope(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    let frame = ((0.020 * sample_rate as f64).round() as usize).max(1);
    let hop = ((0.010 * sample_rate as f64).round() as usize).max(1);
    if samples.len() < frame {
        return Vec::new();
    }
    let mut out = Vec::with_capacity((samples.len() - frame) / hop + 1);
    let mut sum_sq = 0.0f64;
    for i in 0..samples.len() {
        sum_sq += (samples[i] as f64) * (samples[i] as f64);
        if i >= frame {
            sum_sq -= (samples[i - frame] as f64) * (samples[i - frame] as f64);
        }
        if i + 1 >= frame && (i + 1 - frame).is_multiple_of(hop) {
            let rms = (sum_sq / frame as f64).max(0.0).sqrt();
            out.push(20.0 * rms.max(1e-9).log10() as f32);
        }
    }
    out
}

/// The RMS envelope is computed at this rate, in frames per second.
const ENVELOPE_RATE: f64 = 100.0;

/// Measure the residual alignment of two RMS envelopes around a common position.
///
/// Returns the offset in **envelope frames** and the peak's normalised correlation.
/// The search is restricted to `max_lag_frames` either side of zero, so a distant
/// similarity cannot masquerade as a match.
fn envelope_offset(a: &[f32], b: &[f32], max_lag_frames: usize) -> Option<(f64, f32)> {
    if a.len() < 8 || b.len() < 8 {
        return None;
    }
    let corr = crate::matching::dsp::ncc_correlate(a, b);
    if corr.is_empty() {
        return None;
    }
    let mut best_lag = 0i64;
    let mut best_value = f32::NEG_INFINITY;
    for lag in -(max_lag_frames as i64)..=(max_lag_frames as i64) {
        let value = corr.value(lag);
        if value > best_value {
            best_value = value;
            best_lag = lag;
        }
    }
    if !best_value.is_finite() {
        return None;
    }
    let frac = match corr.index_of(best_lag) {
        Some(index) => crate::matching::dsp::parabolic_offset(&corr, index),
        None => 0.0,
    };
    Some((best_lag as f64 + frac, best_value))
}

/// Percentile / max statistics of a residual series.
///
/// The statistics are computed over the **magnitude** of the residuals: what matters
/// is how far the corrected audio is from where the model said it would be, not which
/// side it landed on. The signed value stays available on each point.
pub fn residual_stats(residuals: &[f64]) -> (f64, f64, f64) {
    if residuals.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted: Vec<f64> = residuals.iter().map(|v| v.abs()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = sorted[sorted.len() / 2];
    let p95 = sorted[((sorted.len() as f64 * 0.95).round() as usize).min(sorted.len() - 1)];
    let max = sorted.iter().fold(0.0f64, |m, v| m.max(*v));
    (p50, p95, max)
}

/// Remove the outliers from a residual series, using a median-absolute-deviation rule.
///
/// The RMS-envelope opinion is ambiguous in near-silence: a quiet passage can match a
/// completely different moment of the reference. Such a position produces a residual
/// that is large but not *wildly* so, so a fixed threshold is not enough — the
/// deviation from the median of the series is the signal that matters.
fn reject_outliers(residuals: &[f64]) -> (Vec<f64>, usize) {
    if residuals.len() < 5 {
        return (residuals.to_vec(), 0);
    }
    let mut sorted: Vec<f64> = residuals.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];
    let mut deviations: Vec<f64> = residuals.iter().map(|v| (v - median).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = deviations[deviations.len() / 2];
    // 1.4826 scales the MAD to a standard-deviation equivalent for normally
    // distributed data; the floor keeps the rule from rejecting everything when the
    // series is very tight.
    let threshold = (6.0 * 1.4826 * mad).max(0.020);
    let kept: Vec<f64> = residuals
        .iter()
        .copied()
        .filter(|v| (v - median).abs() <= threshold)
        .collect();
    let rejected = residuals.len() - kept.len();
    (kept, rejected)
}

/// Estimate the fundamental frequency of a mono signal by autocorrelation.
///
/// A deliberately simple, independent tracker: it is only used to confirm that a drift
/// correction of a few ppm has not audibly changed the pitch.
pub fn estimate_f0(samples: &[f32], sample_rate: u32, fmin: f64, fmax: f64) -> Option<f64> {
    let min_lag = (sample_rate as f64 / fmax).floor().max(2.0) as usize;
    let max_lag = (sample_rate as f64 / fmin).ceil().min(samples.len() as f64) as usize;
    if max_lag <= min_lag || samples.len() < max_lag + 1 {
        return None;
    }

    // Energy of the whole window.
    let energy: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    if energy <= 1e-12 {
        return None;
    }

    let mut best_lag = 0usize;
    let mut best = 0.0f64;
    for lag in min_lag..=max_lag {
        let mut acc = 0.0f64;
        for i in 0..(samples.len() - lag) {
            acc += samples[i] as f64 * samples[i + lag] as f64;
        }
        let norm = acc / energy;
        if norm > best {
            best = norm;
            best_lag = lag;
        }
    }
    if best_lag == 0 || best < 0.45 {
        return None;
    }

    // Parabolic refinement around the winning lag.
    let refined = if best_lag > min_lag && best_lag < max_lag {
        let mut vals = [0.0f64; 3];
        for (i, lag) in [best_lag - 1, best_lag, best_lag + 1].iter().enumerate() {
            let mut acc = 0.0f64;
            for k in 0..(samples.len() - lag) {
                acc += samples[k] as f64 * samples[k + lag] as f64;
            }
            vals[i] = acc / energy;
        }
        let denom = vals[0] - 2.0 * vals[1] + vals[2];
        let delta = if denom.abs() > 1e-12 {
            (0.5 * (vals[0] - vals[2]) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        best_lag as f64 + delta
    } else {
        best_lag as f64
    };

    Some(sample_rate as f64 / refined)
}

/// Estimate the fundamental frequency of a mono signal, searched near a known value.
///
/// The autocorrelation of polyphonic material has many peaks. Letting two signals pick
/// their own global maximum can put them on different harmonics, which would make a
/// measured pitch change meaningless. Anchoring the search near a reference frequency
/// compares like with like: it answers "has *this* partial moved?".
///
/// `tolerance` is the relative half-width of the search band.
pub fn estimate_f0_near(
    samples: &[f32],
    sample_rate: u32,
    near_hz: f64,
    tolerance: f64,
) -> Option<f64> {
    if near_hz <= 0.0 || tolerance <= 0.0 || samples.len() < 16 {
        return None;
    }
    let near_lag = sample_rate as f64 / near_hz;
    let min_lag = (near_lag / (1.0 + tolerance)).floor().max(2.0) as usize;
    let max_lag = ((near_lag / (1.0 - tolerance)).ceil() as usize).min(samples.len() - 1);
    if max_lag <= min_lag {
        return None;
    }

    let energy: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    if energy <= 1e-12 {
        return None;
    }

    let mut best_lag = 0usize;
    let mut best = f64::NEG_INFINITY;
    for lag in min_lag..=max_lag {
        let mut acc = 0.0f64;
        for i in 0..(samples.len() - lag) {
            acc += samples[i] as f64 * samples[i + lag] as f64;
        }
        let norm = acc / energy;
        if norm > best {
            best = norm;
            best_lag = lag;
        }
    }
    if best_lag == 0 || !best.is_finite() || best < 0.30 {
        // No confident periodicity near the requested frequency. Reporting a value
        // here would be reporting a different partial.
        return None;
    }

    // Parabolic refinement around the winning lag.
    let refined = if best_lag > min_lag && best_lag < max_lag {
        let mut vals = [0.0f64; 3];
        for (i, lag) in [best_lag - 1, best_lag, best_lag + 1].iter().enumerate() {
            let mut acc = 0.0f64;
            for k in 0..(samples.len() - lag) {
                acc += samples[k] as f64 * samples[k + lag] as f64;
            }
            vals[i] = acc / energy;
        }
        let denom = vals[0] - 2.0 * vals[1] + vals[2];
        let delta = if denom.abs() > 1e-12 {
            (0.5 * (vals[0] - vals[2]) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        best_lag as f64 + delta
    } else {
        best_lag as f64
    };

    Some(sample_rate as f64 / refined)
}
