//! Fine alignment: measure the correspondence point by point.
//!
//! The coarse stage says *where* a source lives; the fine stage measures *exactly* where,
//! at regular intervals along the recording, and reports how much to trust each
//! measurement. Every point carries a confidence and an explicit reason when that
//! confidence is limited — nothing is silently forced to match.

use serde::{Deserialize, Serialize};

use super::dsp::{find_peaks, ncc_correlate, ncc_correlate_masked, parabolic_offset};
use super::model::{LinearModel, TimeMap};
use super::regression::robust_fit;
use super::{Confidence, MatchMethod, MatchPoint, UncertaintyReason};
use crate::features::{Envelope, LogMel};

/// Tunables for the fine stage.
#[derive(Debug, Clone, Copy)]
pub struct FineOptions {
    /// Spacing between probes, in seconds.
    pub hop_secs: f64,
    /// Length of each probe window, in seconds.
    pub window_secs: f64,
    /// Search radius around the model prediction, in seconds.
    pub search_secs: f64,
    /// Minimum fraction of non-quiet frames in a probe window.
    pub min_active_fraction: f64,
    /// Minimum peak separation (in sidelobe standard deviations) for a trusted match.
    pub min_zscore: f32,
    /// Minimum normalised correlation value for a trusted match.
    pub min_peak: f32,
    /// How far a measurement may sit from the fitted line and still support it, in
    /// milliseconds.
    ///
    /// This is the tolerance of the **corroboration test**: a line that most of the
    /// measurements do not support is refused rather than applied. It is set far
    /// above the matcher's own resolution — the envelope resolves a position to
    /// about a millisecond, and the real recordings' residuals stay under ten — and
    /// far below the distance a *wrong* match can be, which is the search radius
    /// around the prediction.
    pub support_tolerance_ms: f64,
}

impl Default for FineOptions {
    fn default() -> Self {
        Self {
            hop_secs: 15.0,
            window_secs: 30.0,
            search_secs: 2.0,
            min_active_fraction: 0.35,
            min_zscore: 3.0,
            min_peak: 0.15,
            support_tolerance_ms: 100.0,
        }
    }
}

/// The result of the fine stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FineResult {
    /// Every measured point, including low-confidence ones.
    pub points: Vec<MatchPoint>,
    /// The model fitted to the confident points.
    pub model: Option<LinearModel>,
    /// Residual statistics of the fitted model, in milliseconds.
    pub residual_p50_ms: f64,
    pub residual_p95_ms: f64,
    pub residual_max_ms: f64,
    /// Number of points that survived the robust fit.
    pub inliers: usize,
    /// Number of points that were measured at all.
    pub measured: usize,
    /// Number of probes that had too little signal to measure.
    pub skipped: usize,
    /// Why the probes were skipped.
    pub skip_reasons: SkipReasons,
    /// Why the fitted model was refused, when it was.
    ///
    /// A model that the measurements do not corroborate is not a measurement, and
    /// applying it would place a recording somewhere it does not belong. When this
    /// is set, [`FineResult::model`] is `None` and the caller keeps the model it
    /// already had — the coarse lock's, which came from an independent estimator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit_rejection: Option<String>,
}

/// A breakdown of why fine probes could not be measured.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SkipReasons {
    /// The probe window was too quiet.
    pub low_energy: usize,
    /// The reference had no material in the searched region.
    pub no_region: usize,
    /// The correlation produced no usable peak.
    pub no_peak: usize,
}

/// Measure the source/reference correspondence around a model prediction.
pub fn fine_match(
    source: &Envelope,
    reference: &Envelope,
    model: &LinearModel,
    options: &FineOptions,
) -> FineResult {
    let mut points: Vec<MatchPoint> = Vec::new();
    let mut skipped = 0usize;
    let mut reasons = SkipReasons::default();

    let rate = source.rate_hz;
    let default_win = (options.window_secs * rate).round() as usize;
    let min_short_frames = (8.0 * rate).round() as usize;

    if default_win < 8 || source.len() < min_short_frames.min(default_win) {
        return FineResult {
            points,
            model: Some(*model),
            residual_p50_ms: 0.0,
            residual_p95_ms: 0.0,
            residual_max_ms: 0.0,
            inliers: 0,
            measured: 0,
            skipped: 0,
            skip_reasons: SkipReasons::default(),
            fit_rejection: None,
        };
    }

    // Recordings between 8 s and the 30 s default window would yield zero fine
    // probes at the full window size. Scale the probe window to half the source
    // (at least 4 s) with a 40% hop so short clips still produce several
    // overlapping probes while sub-8 s fragments continue to report no fine points.
    let short_clip = source.len() < default_win;
    let search_secs = if short_clip {
        options.search_secs.max(4.0)
    } else {
        options.search_secs
    };
    let (win, hop) = if short_clip {
        let short_win = (source.len() / 3)
            .max((4.0 * rate).round() as usize)
            .min(source.len())
            .max(8);
        let short_hop = ((short_win as f64) * 0.4).max(1.0);
        (short_win, short_hop)
    } else {
        (default_win, (options.hop_secs * rate).max(1.0))
    };

    let half = win / 2;
    for p in probe_positions(source.len(), half, hop) {
        let window = &source.norm[p - half..p + half];

        // The probe must contain real signal, not just the noise floor.
        let active = source.quiet[p - half..p + half]
            .iter()
            .filter(|q| !**q)
            .count() as f64
            / win as f64;
        if active < options.min_active_fraction {
            skipped += 1;
            reasons.low_energy += 1;
            continue;
        }
        // Reference region around the model prediction.
        let predicted = model.reference_time(p as f64 / rate);
        let region_start = predicted - (half as f64 / rate) - search_secs;
        let region_end = predicted + (half as f64 / rate) + search_secs;
        let i0 = reference.index_at(region_start.max(0.0));
        let i1 = reference
            .index_at(region_end)
            .min(reference.len().saturating_sub(1));
        if i1 <= i0 {
            skipped += 1;
            reasons.no_region += 1;
            continue;
        }
        let region = &reference.norm[i0..=i1];
        let region_start_time = i0 as f64 / rate;

        // The silent part of the probe window carries no information, and comparing it
        // against the reference's music would both weaken the correlation and drag the
        // peak away from the true correspondence. The same frames are excluded from the
        // reference's side of the comparison, so the two are always compared over the
        // same material.
        let mask = &source.quiet[p - half..p + half];
        let corr = ncc_correlate_masked(window, region, Some(mask));
        let min_sep = if short_clip {
            ((search_secs * rate * 0.25) as i64).max(1)
        } else {
            ((options.search_secs * rate * 0.5) as i64).max(1)
        };
        let peaks = find_peaks(&corr, 1, min_sep);
        let Some(peak) = peaks.first() else {
            skipped += 1;
            reasons.no_peak += 1;
            continue;
        };

        // Refine to sub-frame accuracy.
        let frac = parabolic_offset(&corr, peak.index);
        let lag_frames = peak.lag as f64 + frac;
        let reference_time = region_start_time + lag_frames / rate + half as f64 / rate;

        let (score, reason) = score_point(peak.ncc, peak.zscore, active, options);
        points.push(MatchPoint {
            source_time: p as f64 / rate,
            reference_time,
            lag: reference_time - p as f64 / rate,
            residual_ms: 0.0,
            confidence: Confidence::new(score, reason),
            method: MatchMethod::FineEnvelope,
            note: String::new(),
        });
    }

    finish(points, skipped, reasons, options)
}

/// The positions in the source that the fine stage probes.
///
/// The probes are evenly spaced, and each one's window has to lie **inside** the
/// source. That last requirement means the final half-window of a recording is
/// never reached: on a three-hour recording the tail is a rounding error, but on
/// a ninety-second one it is a sixth of the material — and it is the material a
/// listener hears last. The tail is therefore probed as well, clamped to the end
/// of the recording, whenever the clamped probe is at least half a hop away from
/// the last regular one. On a long recording the two are the same probe to
/// within a fraction of a hop, and nothing changes.
fn probe_positions(len: usize, half: usize, hop: f64) -> Vec<usize> {
    let mut positions: Vec<usize> = Vec::new();
    let mut probe = half as f64;
    while (probe as usize) + half < len {
        positions.push(probe as usize);
        probe += hop;
    }

    let hop_frames = hop.round() as usize;
    if hop_frames == 0 {
        return positions;
    }
    let last = positions.last().copied().unwrap_or(0);
    let tail = len.saturating_sub(half);
    if tail > last && (tail - last) * 2 >= hop_frames {
        positions.push(tail);
    }
    positions
}

/// Fuse the evidence for a single point into a confidence score.
fn score_point(
    peak_value: f32,
    zscore: f32,
    active_fraction: f64,
    options: &FineOptions,
) -> (f32, UncertaintyReason) {
    let separation = ((zscore - 1.5) / 8.0).clamp(0.0, 1.0);
    let strength = peak_value.clamp(0.0, 1.0);
    let energy = active_fraction.clamp(0.0, 1.0) as f32;

    let mut score = 0.45 * separation + 0.35 * strength + 0.20 * energy;

    let reason = if active_fraction < options.min_active_fraction {
        UncertaintyReason::LowEnergy
    } else if zscore < options.min_zscore {
        UncertaintyReason::LowPeakSeparation
    } else if peak_value < options.min_peak {
        UncertaintyReason::WeakAgreement
    } else {
        UncertaintyReason::None
    };
    if reason != UncertaintyReason::None {
        score = score.min(0.49);
    }
    (score, reason)
}

/// Fit the robust model to the measured points and compute residual statistics.
fn finish(
    mut points: Vec<MatchPoint>,
    skipped: usize,
    skip_reasons: SkipReasons,
    options: &FineOptions,
) -> FineResult {
    let measured = points.len();

    // Fit using the confident points only. Falling back to *all* the points when there
    // are few confident ones would put the very measurements that were rejected as
    // unreliable into the model, and a fit built on those is worse than no fit at all:
    // it looks like a measurement, it is not.
    let confident_indices: Vec<usize> = points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.confidence.is_trustworthy())
        .map(|(i, _)| i)
        .collect();
    let confident: Vec<(f64, f64)> = confident_indices
        .iter()
        .map(|&i| (points[i].source_time, points[i].reference_time))
        .collect();

    // Three points is the minimum for a line to mean anything: two would fix the slope
    // exactly and could not survive a single wrong measurement.
    let raw_fit = if confident.len() >= 3 {
        robust_fit(&confident, 3.0, 6)
    } else {
        None
    };

    // A fit has to be **corroborated** before it may be applied. The robust fit
    // rejects an outlier by measuring the median deviation from the line it has
    // found — but with only three or four points the median of the pairwise slopes
    // can itself be one of the outlier's own slopes, and the resulting line is then
    // supported by the outlier and one good point rather than by the good points.
    // That failure is silent: the fit looks perfectly ordinary, and the recording it
    // describes sits seconds away from where it belongs.
    //
    // The test is therefore about *support* rather than about the estimator: the line
    // has to be within the support tolerance of most of the measurements that
    // produced it. At most a third of them may disagree.
    let tolerance = options.support_tolerance_ms / 1000.0;
    let mut fit_rejection: Option<String> = None;
    let fit = match raw_fit {
        None => None,
        Some(f) => {
            let (support, count) = corroboration_support(&confident, &f.model(), tolerance);
            let allowed = confident.len().saturating_sub(1) / 3;
            if count + allowed >= confident.len() {
                Some(f)
            } else {
                fit_rejection = Some(format!(
                    "only {count} of the {} confident measurements support the fitted line \
                     (at most {allowed} of them may disagree); the line was refused and the \
                     placement rests on the coarse lock alone",
                    confident.len()
                ));
                // The measurements that do not support the line are not trustworthy:
                // the line itself is what says so.
                for (k, &i) in confident_indices.iter().enumerate() {
                    if !support.get(k).copied().unwrap_or(true) {
                        points_outlier(&mut points, i);
                    }
                }
                None
            }
        }
    };
    let model = fit.as_ref().map(|f| f.model());

    let mut out = points;
    if let Some(f) = &fit {
        for p in out.iter_mut() {
            // "Measured minus model": a positive residual means the measurement sits
            // later in the reference than the model predicts.
            p.residual_ms = (p.reference_time - (f.slope * p.source_time + f.intercept)) * 1000.0;
        }
        // Mark the points the robust fit rejected. The inlier mask is indexed by the
        // *fitted* points, so it has to be mapped back through `confident_indices`.
        for (k, &i) in confident_indices.iter().enumerate() {
            if !f.inliers.get(k).copied().unwrap_or(true) {
                out[i].confidence.score = out[i].confidence.score.min(0.3);
                out[i].confidence.reason = UncertaintyReason::Outlier;
            }
        }
    }

    let residuals: Vec<f64> = out
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| p.residual_ms)
        .collect();
    let (p50, p95, max) = residual_stats(&residuals);

    FineResult {
        points: out,
        model,
        residual_p50_ms: p50,
        residual_p95_ms: p95,
        residual_max_ms: max,
        inliers: fit
            .as_ref()
            .map_or(0, |f| f.inliers.iter().filter(|i| **i).count()),
        measured,
        skipped,
        skip_reasons,
        fit_rejection,
    }
}

/// Which measurements support a fitted line, and how many of them there are.
///
/// A measurement supports the line when it lies within `tolerance` of it. The
/// tolerance is in seconds and is the same for every measurement.
pub fn corroboration_support(
    points: &[(f64, f64)],
    fit: &LinearModel,
    tolerance: f64,
) -> (Vec<bool>, usize) {
    let support: Vec<bool> = points
        .iter()
        .map(|(x, y)| (fit.reference_time(*x) - y).abs() <= tolerance)
        .collect();
    let count = support.iter().filter(|s| **s).count();
    (support, count)
}

/// Mark one measurement as an outlier, so that it can no longer drive a correction.
fn points_outlier(points: &mut [MatchPoint], index: usize) {
    if let Some(point) = points.get_mut(index) {
        point.confidence.score = point.confidence.score.min(0.3);
        point.confidence.reason = UncertaintyReason::Outlier;
    }
}

/// Absolute residual statistics of a residual series, in milliseconds.
///
/// The statistics are taken over the **absolute** residuals: a residual of -3 ms and one
/// of +3 ms are the same disagreement with the model, and a report that showed the
/// signed median would hide a bias that the maximum reveals.
pub fn residual_stats(residuals: &[f64]) -> (f64, f64, f64) {
    if residuals.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted: Vec<f64> = residuals.iter().map(|r| r.abs()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = sorted[sorted.len() / 2];
    let p95 = sorted[((sorted.len() as f64 * 0.95).round() as usize).min(sorted.len() - 1)];
    let max = sorted[sorted.len() - 1];
    (p50, p95, max)
}

/// Independent spectral (log-mel) measurement of the same correspondence.
///
/// This deliberately uses a **different feature family** from the envelope that fitted the
/// model, so it can act as a cross-check: agreement between the two is evidence, and
/// disagreement is a warning rather than something to be averaged away.
pub fn spectral_offsets(
    source: &LogMel,
    reference: &LogMel,
    model: &LinearModel,
    options: &FineOptions,
) -> Vec<MatchPoint> {
    let mut points = Vec::new();
    // The two files' log-mel frame rates are equal only by accident. The hop is
    // rounded to whole samples, so a 44.1 kHz file's frames are 32.0017 ms apart
    // and a 48 kHz file's are 32 ms apart: the nominal rates are identical and the
    // actual rates are 143 ppm apart. A frame index must therefore be converted
    // with the rate of the file it indexes — the source's indices with the
    // source's rate, the reference's with the reference's.
    let source_rate = source.rate_hz;
    let reference_rate = reference.rate_hz;
    let win = (options.window_secs * source_rate).round() as usize;

    if win < 8 {
        return points;
    }
    let half = win / 2;

    let mut probe = half as f64;
    while (probe as usize) + half < source.n_frames {
        let p = probe as usize;
        // The probe's own time is the time of the frame that is actually read. The
        // probe position is a fractional frame index and is truncated to the frame
        // that is correlated, so the two differ by up to one frame — and a frame is
        // 32 ms. Reporting the ideal position instead would put a 32 ms sawtooth on
        // every measurement.
        let source_time = p as f64 / source_rate;
        let predicted = model.reference_time(source_time);
        let region_start = predicted - (half as f64 / source_rate) - options.search_secs;
        let region_end = predicted + (half as f64 / source_rate) + options.search_secs;
        let r0 = reference.index_at(region_start.max(0.0));
        let r1 = reference
            .index_at(region_end)
            .min(reference.n_frames.saturating_sub(1));
        if r1 <= r0 {
            probe += options.hop_secs * source_rate;
            continue;
        }
        let region_len = r1 - r0 + 1;

        // Sum the normalised cross-correlation of every band.
        let corr_len = win + region_len - 1;
        let mut total = vec![0.0f32; corr_len];
        let mut valid = false;
        for band in 0..source.n_bands {
            // The band offset has to be applied to *both* ends of the slice, otherwise
            // the range runs off the end of the band-major array for every band > 0.
            let q_start = band * source.n_frames + (p - half);
            let r_start = band * reference.n_frames + r0;
            // Defensive: a malformed feature must never panic the whole analysis.
            if q_start + win > source.values.len() || r_start + region_len > reference.values.len()
            {
                valid = false;
                break;
            }
            let q = &source.values[q_start..q_start + win];
            let r = &reference.values[r_start..r_start + region_len];
            let qn = normalise_slice(q);
            let rn = normalise_slice(r);
            let corr = ncc_correlate(&qn, &rn);
            if corr.len() != total.len() {
                continue;
            }
            for (i, v) in corr.values.iter().enumerate() {
                total[i] += v;
            }
            valid = true;
        }
        if !valid {
            probe += options.hop_secs * source_rate;
            continue;
        }
        let scale = 1.0 / source.n_bands as f32;
        for v in total.iter_mut() {
            *v *= scale;
        }
        // The spectral cross-check ranks candidates by the summed band correlations;
        // the overlap weighting that the envelope matcher uses does not apply here.
        let corr = super::dsp::Correlation {
            fixed: total.clone(),
            values: total,
            lag_min: -(win as i64 - 1),
            lag_max: region_len as i64,
        };
        let min_sep = ((options.search_secs * reference_rate * 0.5) as i64).max(1);
        let peaks = find_peaks(&corr, 1, min_sep);
        if let Some(peak) = peaks.first() {
            let frac = parabolic_offset(&corr, peak.index);
            let lag_frames = peak.lag as f64 + frac;
            // The reference's own frames, converted with the reference's own rate.
            let reference_time = (r0 as f64 + lag_frames + half as f64) / reference_rate;
            let (score, reason) = score_point(peak.ncc, peak.zscore, 1.0, options);
            points.push(MatchPoint {
                source_time,
                reference_time,
                lag: reference_time - source_time,
                residual_ms: 0.0,
                confidence: Confidence::new(score, reason),
                method: MatchMethod::FineSpectral,
                note: String::new(),
            });
        }
        probe += options.hop_secs * source_rate;
    }
    points
}

/// Zero-mean, unit-variance normalisation of a slice.
fn normalise_slice(values: &[f32]) -> Vec<f32> {
    if values.is_empty() {
        return Vec::new();
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32;
    let std = var.sqrt();
    if std < 1e-9 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v - mean) / std).collect()
}

/// Build a continuous time map from a linear model.
pub fn time_map_from_model(model: LinearModel, source_duration: f64) -> TimeMap {
    TimeMap::from_linear(model, source_duration)
}
