//! Cross-correlation primitives shared by the matching and verification stages.
//!
//! All correlations in this crate use one convention:
//!
//! ```text
//! corr(lag) = Σ_n query[n] * reference[n + lag]
//! ```
//!
//! i.e. `lag` is the position in the **reference** where `query[0]` lands. Positive lag
//! means the query appears *later* in the reference.

use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;

/// Next power of two greater than or equal to `n` (minimum 1).
pub fn next_pow2(n: usize) -> usize {
    n.max(1).next_power_of_two()
}

/// A correlation curve indexed by lag.
///
/// Only lags in `[lag_min, lag_max]` are meaningful: outside that range the two signals
/// overlap by less than half of the shorter one, and a normalised correlation over a
/// very short overlap is dominated by whatever periodicity the material happens to have.
/// Those lags are marked invalid rather than reported, because a spurious edge peak is
/// exactly how a correlation matcher latches onto the wrong place in a long recording.
#[derive(Debug, Clone)]
pub struct Correlation {
    /// Overlap-normalised correlation in `[-1, 1]`; this is the *match quality* and is
    /// what the confidence score is built from.
    pub values: Vec<f32>,
    /// Matched-filter score with a fixed normalisation (the whole signal's energy).
    /// This is used to *rank* candidate lags: a short overlap cannot produce a strong
    /// value here, so a spurious edge peak cannot outrank a real match.
    pub fixed: Vec<f32>,
    /// First lag in `values`.
    pub lag_min: i64,
    /// Last lag in `values` that is considered reliable.
    pub lag_max: i64,
}

impl Correlation {
    /// Number of lag positions.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` when the curve is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Matched-filter score at a given lag.
    pub fn fixed_value(&self, lag: i64) -> f32 {
        let idx = lag - self.lag_min;
        if idx < 0 || idx as usize >= self.fixed.len() {
            0.0
        } else {
            self.fixed[idx as usize]
        }
    }

    /// Value at a given lag (returns `0.0` outside the computed range).
    pub fn value(&self, lag: i64) -> f32 {
        let idx = lag - self.lag_min;
        if idx < 0 || idx as usize >= self.values.len() {
            0.0
        } else {
            self.values[idx as usize]
        }
    }

    /// Index into `values` for a given lag.
    pub fn index_of(&self, lag: i64) -> Option<usize> {
        let idx = lag - self.lag_min;
        if idx < 0 || idx as usize >= self.values.len() {
            None
        } else {
            Some(idx as usize)
        }
    }

    /// `true` when the lag has enough overlapping material to be meaningful.
    pub fn is_reliable(&self, lag: i64) -> bool {
        lag >= self.lag_min && lag <= self.lag_max
    }
}

/// A detected correlation peak.
#[derive(Debug, Clone, Copy)]
pub struct Peak {
    /// Lag in samples.
    pub lag: i64,
    /// Matched-filter score at the peak (used for ranking).
    pub value: f32,
    /// Overlap-normalised correlation at the peak (used for confidence).
    pub ncc: f32,
    /// How many standard deviations the peak stands above the sidelobes.
    pub zscore: f32,
    /// Index into the correlation curve.
    pub index: usize,
}

/// Normalised cross-correlation of `query` against `reference` over every possible lag.
///
/// Returns a curve whose lag range is `[-query.len() + 1, reference.len() - 1]`.
///
/// The correlation is normalised by the energy of the overlapping portion of both signals,
/// so a match in a loud passage is not scored higher than the same match in a quiet one.
pub fn ncc_correlate(query: &[f32], reference: &[f32]) -> Correlation {
    ncc_correlate_masked(query, reference, None)
}

/// Normalised cross-correlation with some of the query's samples excluded.
///
/// `mask` holds one entry per query sample; `true` means "this sample carries no
/// information". An excluded sample contributes to neither the correlation nor the
/// normalisation, and the reference's energy is measured over exactly the same
/// positions — so the two sides are always compared over the same material.
///
/// This is what makes a stretch of silence safe to leave in the recording. Without it,
/// the silent part of a probe window is compared against music in the reference, which
/// both corrupts the peak and drags the peak away from the true correspondence.
pub fn ncc_correlate_masked(
    query: &[f32],
    reference: &[f32],
    mask: Option<&[bool]>,
) -> Correlation {
    let la = query.len();
    let lb = reference.len();
    if la == 0 || lb == 0 {
        return Correlation {
            values: Vec::new(),
            fixed: Vec::new(),
            lag_min: 0,
            lag_max: 0,
        };
    }

    let n = next_pow2(la + lb);
    let lag_min = -(la as i64 - 1);
    let total = la + lb - 1;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    let mut a: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
    let mut b: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
    for (i, &v) in query.iter().enumerate() {
        // An excluded sample contributes nothing to the correlation either.
        let excluded = mask.is_some_and(|m| m.get(i).copied().unwrap_or(false));
        a[i] = Complex32::new(if excluded { 0.0 } else { v }, 0.0);
    }
    for (i, &v) in reference.iter().enumerate() {
        b[i] = Complex32::new(v, 0.0);
    }
    fft.process(&mut a);
    fft.process(&mut b);
    // conj(A) * B  ->  IFFT gives sum_n query[n] * reference[n + j], so the peak sits at
    // the offset of the query inside the reference.
    for i in 0..n {
        a[i] = a[i].conj() * b[i];
    }
    ifft.process(&mut a);
    let scale = 1.0 / n as f32;

    // The reference's energy under the *shifted* mask: e[j] = sum_n mask[n] * ref[n+j]^2.
    // It is a second correlation of the mask against the squared reference, so it costs
    // one more transform pair rather than a loop over every lag.
    let masked = mask.is_some();
    let mut masked_ref_energy: Vec<f32> = Vec::new();
    if masked {
        let mask = mask.expect("checked above");
        let mut m: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
        let mut r2: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
        for (i, slot) in m.iter_mut().enumerate().take(la) {
            if !mask.get(i).copied().unwrap_or(false) {
                *slot = Complex32::new(1.0, 0.0);
            }
        }
        for (i, &v) in reference.iter().enumerate() {
            r2[i] = Complex32::new(v * v, 0.0);
        }
        fft.process(&mut m);
        fft.process(&mut r2);
        for i in 0..n {
            m[i] = m[i].conj() * r2[i];
        }
        ifft.process(&mut m);
        masked_ref_energy = (0..total)
            .map(|k| {
                let j = k as i64 + lag_min;
                let idx = if j >= 0 {
                    j as usize
                } else {
                    (n as i64 + j) as usize
                };
                m[idx].re * scale
            })
            .collect();
    }

    // Prefix sums of squares for energy normalisation.
    let mut pre_q = vec![0.0f64; la + 1];
    let mut pre_q_masked = vec![0.0f64; la + 1];
    let mut pre_mask_count = vec![0usize; la + 1];
    for i in 0..la {
        let v = query[i] as f64;
        pre_q[i + 1] = pre_q[i] + v * v;
        let excluded = mask.is_some_and(|m| m.get(i).copied().unwrap_or(false));
        pre_q_masked[i + 1] = pre_q_masked[i] + if excluded { 0.0 } else { v * v };
        pre_mask_count[i + 1] = pre_mask_count[i] + usize::from(!excluded);
    }
    let mut pre_r = vec![0.0f64; lb + 1];
    for i in 0..lb {
        pre_r[i + 1] = pre_r[i] + (reference[i] as f64).powi(2);
    }

    let lag_min = -(la as i64 - 1);
    let total = la + lb - 1;
    let mut values = vec![0.0f32; total];
    let mut fixed = vec![0.0f32; total];
    let total_q = if masked {
        pre_q_masked[la]
    } else {
        pre_q[la] - pre_q[0]
    }
    .max(0.0);
    let total_r = (pre_r[lb] - pre_r[0]).max(0.0);
    let fixed_denom = (total_q * total_r).sqrt();

    // The overlap must be at least half of the shorter signal for the normalised
    // correlation to mean anything. With a mask, the overlap counts the samples that
    // were actually compared.
    let usable_samples = if masked { pre_mask_count[la] } else { la };
    let min_overlap = (usable_samples.min(lb) / 2).max(1);
    let mut first_valid: Option<i64> = None;
    let mut last_valid: i64 = lag_min;

    for (k, slot) in values.iter_mut().enumerate() {
        let lag = lag_min + k as i64;
        let raw = if lag >= 0 {
            a[lag as usize].re * scale
        } else {
            a[(n as i64 + lag) as usize].re * scale
        };

        // Overlapping index ranges.
        let lo_q = (-lag).max(0) as usize;
        let hi_q = ((lb as i64 - lag) as usize).min(la);
        if hi_q <= lo_q {
            *slot = 0.0;
            continue;
        }
        let overlap = if masked {
            pre_mask_count[hi_q] - pre_mask_count[lo_q]
        } else {
            hi_q - lo_q
        };
        if overlap == 0 {
            *slot = 0.0;
            continue;
        }
        let sum_q = if masked {
            pre_q_masked[hi_q] - pre_q_masked[lo_q]
        } else {
            pre_q[hi_q] - pre_q[lo_q]
        };
        let sum_r = if masked {
            masked_ref_energy[k].max(0.0) as f64
        } else {
            let lo_r = (lo_q as i64 + lag) as usize;
            let hi_r = (hi_q as i64 + lag) as usize;
            pre_r[hi_r] - pre_r[lo_r]
        };
        let denom = (sum_q * sum_r).sqrt();
        *slot = if denom > 1e-12 {
            (raw as f64 / denom) as f32
        } else {
            0.0
        };

        if overlap >= min_overlap {
            if first_valid.is_none() {
                first_valid = Some(lag);
            }
            last_valid = lag;
            fixed[k] = if fixed_denom > 1e-12 {
                (raw as f64 / fixed_denom) as f32
            } else {
                0.0
            };
        } else {
            *slot = 0.0;
        }
    }

    Correlation {
        values,
        fixed,
        lag_min,
        lag_max: if first_valid.is_some() {
            last_valid
        } else {
            lag_min
        },
    }
}

/// Silence in the middle of a recording is not an error, it is a fact about the
/// recording. This module's job is to make sure it is *reported*, not papered over.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_masked_sample_contributes_nothing_to_the_correlation() {
        let query: Vec<f32> = (0..200).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut reference = query.clone();
        // Replace a quarter of the reference with something unrelated.
        for v in reference.iter_mut().skip(50).take(50) {
            *v = 0.0;
        }

        // The same corruption in the query, masked out, must give the same peak.
        let mut masked_query = query.clone();
        let mut mask = vec![false; 200];
        for (i, v) in masked_query.iter_mut().enumerate().skip(50).take(50) {
            *v = 0.0;
            mask[i] = true;
        }

        let plain = ncc_correlate(&masked_query, &reference);
        let masked = ncc_correlate_masked(&masked_query, &reference, Some(&mask));

        let plain_best = plain
            .values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| plain.lag_min + i as i64);
        let masked_best = masked
            .values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| masked.lag_min + i as i64);

        assert_eq!(plain_best, Some(0), "the unmasked peak moved");
        assert_eq!(masked_best, Some(0), "the masked peak moved");
        assert!(
            masked.values[(-masked.lag_min) as usize] > 0.99,
            "the masked correlation is not a clean match: {}",
            masked.values[(-masked.lag_min) as usize]
        );
    }
}

/// Find the strongest correlation peaks, with a minimum separation between them.
///
/// Each peak carries a z-score relative to the sidelobe distribution (the curve with the
/// peak's neighbourhood removed), which is the primary evidence that a match is real
/// rather than a periodic artefact of the music.
///
/// The peaks are returned **strongest first**. A caller that wants the best match takes
/// the first one; a caller that wants alternatives takes the rest.
pub fn find_peaks(corr: &Correlation, max_peaks: usize, min_separation: i64) -> Vec<Peak> {
    if corr.len() < 3 {
        return Vec::new();
    }
    let mut candidates: Vec<usize> = Vec::new();
    for k in 1..corr.len() - 1 {
        let lag = corr.lag_min + k as i64;
        // Lags with too little overlap are not evidence of anything.
        if !corr.is_reliable(lag) {
            continue;
        }
        if corr.fixed[k] > corr.fixed[k - 1] && corr.fixed[k] >= corr.fixed[k + 1] {
            candidates.push(k);
        }
    }
    candidates.sort_unstable_by(|&x, &y| {
        corr.fixed[y]
            .partial_cmp(&corr.fixed[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut picked: Vec<usize> = Vec::new();
    for k in candidates {
        if picked.len() >= max_peaks {
            break;
        }
        let lag = corr.lag_min + k as i64;
        if picked
            .iter()
            .all(|&p| (corr.lag_min + p as i64 - lag).abs() >= min_separation)
        {
            picked.push(k);
        }
    }
    // Strongest first: the caller wants the best match at index zero.
    picked.sort_unstable_by(|&x, &y| {
        corr.fixed[y]
            .partial_cmp(&corr.fixed[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    picked
        .into_iter()
        .map(|k| {
            let lag = corr.lag_min + k as i64;
            let (mean, std) = sidelobe_stats(corr, k, min_separation);
            let z = if std > 1e-9 {
                (corr.fixed[k] - mean) / std
            } else {
                0.0
            };
            Peak {
                lag,
                value: corr.fixed[k],
                ncc: corr.values[k],
                zscore: z,
                index: k,
            }
        })
        .collect()
}

/// Find the strongest correlation peaks, ranked by the **overlap-normalised** value.
///
/// This is the criterion the **coarse** stage uses, and it differs from
/// [`find_peaks`] deliberately.
///
/// `find_peaks` ranks by the matched-filter score with a *global* normalisation,
/// which is the right criterion when a short overlap must not outrank a real
/// match. It is the wrong criterion when the reference's own level varies over the
/// compared span: the matched-filter sum is linear in the reference's amplitude,
/// so it is largest where the reference happens to be most dynamic, not where the
/// two envelopes agree best. Measured against a synthetic fixture with a known
/// correspondence, that bias put the coarse lock 144 seconds away from the truth
/// on a ninety-second source whose music was split by a recorder gap.
///
/// The overlap-normalised value divides the same sum by the energy of exactly the
/// material that was compared, so it measures the *shape* agreement — which is
/// what the envelope matcher is looking for. The short-overlap edge artefacts
/// that made this project abandon the normalised ranking are excluded here by the
/// same reliability rule `find_peaks` uses: a lag whose overlap is below half of
/// the usable samples is not evidence of anything.
///
/// The peaks are returned **strongest first**, on the normalised value.
pub fn find_peaks_by_ncc(corr: &Correlation, max_peaks: usize, min_separation: i64) -> Vec<Peak> {
    if corr.len() < 3 {
        return Vec::new();
    }
    let mut candidates: Vec<usize> = Vec::new();
    for k in 1..corr.len() - 1 {
        let lag = corr.lag_min + k as i64;
        // Lags with too little overlap are not evidence of anything.
        if !corr.is_reliable(lag) {
            continue;
        }
        if corr.values[k] > corr.values[k - 1] && corr.values[k] >= corr.values[k + 1] {
            candidates.push(k);
        }
    }
    candidates.sort_unstable_by(|&x, &y| {
        corr.values[y]
            .partial_cmp(&corr.values[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut picked: Vec<usize> = Vec::new();
    for k in candidates {
        if picked.len() >= max_peaks {
            break;
        }
        let lag = corr.lag_min + k as i64;
        if picked
            .iter()
            .all(|&p| (corr.lag_min + p as i64 - lag).abs() >= min_separation)
        {
            picked.push(k);
        }
    }

    picked
        .into_iter()
        .map(|k| {
            let lag = corr.lag_min + k as i64;
            let (mean, std) = sidelobe_stats_on(&corr.values, corr, k, min_separation);
            let z = if std > 1e-9 {
                (corr.values[k] - mean) / std
            } else {
                0.0
            };
            Peak {
                lag,
                value: corr.fixed[k],
                ncc: corr.values[k],
                zscore: z,
                index: k,
            }
        })
        .collect()
}

/// Mean and standard deviation of the correlation curve with the peak neighbourhood
/// and the unreliable lags removed.
pub fn sidelobe_stats(corr: &Correlation, peak_index: usize, min_separation: i64) -> (f32, f32) {
    sidelobe_stats_on(&corr.fixed, corr, peak_index, min_separation)
}

/// Mean and standard deviation of a given curve with the peak neighbourhood and the
/// unreliable lags removed.
fn sidelobe_stats_on(
    curve: &[f32],
    corr: &Correlation,
    peak_index: usize,
    min_separation: i64,
) -> (f32, f32) {
    let peak_lag = corr.lag_min + peak_index as i64;
    let mut count = 0u64;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    for (k, &v) in curve.iter().enumerate() {
        let lag = corr.lag_min + k as i64;
        if (lag - peak_lag).abs() < min_separation {
            continue;
        }
        if !corr.is_reliable(lag) {
            continue;
        }
        count += 1;
        sum += v as f64;
        sum_sq += (v as f64) * (v as f64);
    }
    if count == 0 {
        return (0.0, 0.0);
    }
    let mean = sum / count as f64;
    let var = (sum_sq / count as f64 - mean * mean).max(0.0);
    (mean as f32, var.sqrt() as f32)
}

/// Sub-sample peak location by parabolic interpolation over the three samples around `index`.
///
/// Returns the offset in `[-0.5, 0.5]` from `index`.
pub fn parabolic_offset(corr: &Correlation, index: usize) -> f64 {
    if index == 0 || index + 1 >= corr.len() {
        return 0.0;
    }
    let y0 = corr.values[index - 1] as f64;
    let y1 = corr.values[index] as f64;
    let y2 = corr.values[index + 1] as f64;
    let denom = y0 - 2.0 * y1 + y2;
    if denom.abs() < 1e-12 {
        return 0.0;
    }
    let delta = 0.5 * (y0 - y2) / denom;
    delta.clamp(-0.5, 0.5)
}

/// Result of a waveform GCC-PHAT measurement.
#[derive(Debug, Clone, Copy)]
pub struct PhatResult {
    /// Measured lag in samples. The convention is `a[n] ≈ b[n + lag]`: a **positive**
    /// lag means `b` has to be advanced to match `a`, i.e. `b` is running late.
    pub lag: f64,
    /// Peak value of the PHAT-weighted correlation.
    pub peak: f32,
    /// Height of the peak above the sidelobe mean, in standard deviations.
    pub zscore: f32,
}

/// GCC-PHAT: a waveform-domain delay estimate, independent of the envelope feature.
///
/// This is the *second opinion* used by the holdout verifier: the alignment model is fitted
/// with RMS-envelope correlation, while this measurement works on the raw waveform with
/// phase-based weighting, so agreement between the two is real evidence.
pub fn gcc_phat(a: &[f32], b: &[f32], max_lag: usize) -> Option<PhatResult> {
    let n = next_pow2(a.len().max(b.len()) * 2);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    let mut fa: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
    let mut fb: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); n];
    for (i, &v) in a.iter().enumerate() {
        fa[i] = Complex32::new(v, 0.0);
    }
    for (i, &v) in b.iter().enumerate() {
        fb[i] = Complex32::new(v, 0.0);
    }
    fft.process(&mut fa);
    fft.process(&mut fb);

    let mut cross = vec![Complex32::new(0.0, 0.0); n];
    for i in 0..n {
        let p = fa[i] * fb[i].conj();
        let mag = p.norm();
        // Phase transform: keep the phase, flatten the magnitude.
        cross[i] = if mag > 1e-12 {
            Complex32::new(p.re / mag, p.im / mag)
        } else {
            Complex32::new(0.0, 0.0)
        };
    }
    ifft.process(&mut cross);
    let scale = 1.0 / n as f32;

    let mut best_idx = 0usize;
    let mut best_val = f32::NEG_INFINITY;
    let max_lag = max_lag.min(n / 2 - 1);
    for k in 0..=max_lag {
        let v = cross[k].re * scale;
        if v > best_val {
            best_val = v;
            best_idx = k;
        }
        if k > 0 {
            let v = cross[n - k].re * scale;
            if v > best_val {
                best_val = v;
                best_idx = n - k;
            }
        }
    }
    if !best_val.is_finite() {
        return None;
    }

    // Sidelobe statistics over the whole curve except the peak neighbourhood.
    let peak_lag = if best_idx > n / 2 {
        best_idx as i64 - n as i64
    } else {
        best_idx as i64
    };
    let mut count = 0u64;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    for (k, value) in cross.iter().take(n).enumerate() {
        let lag = if k > n / 2 {
            k as i64 - n as i64
        } else {
            k as i64
        };
        if (lag - peak_lag).abs() < 64 {
            continue;
        }
        let v = value.re * scale;
        count += 1;
        sum += v as f64;
        sum_sq += (v as f64) * (v as f64);
    }
    let (mean, std) = if count > 0 {
        let m = sum / count as f64;
        let var = (sum_sq / count as f64 - m * m).max(0.0);
        (m as f32, var.sqrt() as f32)
    } else {
        (0.0, 0.0)
    };
    let z = if std > 1e-9 {
        (best_val - mean) / std
    } else {
        0.0
    };

    let raw_offset = if best_idx > 0 && best_idx + 1 < n {
        parabolic_scalar(
            cross[best_idx - 1].re,
            cross[best_idx].re,
            cross[best_idx + 1].re,
        )
    } else {
        0.0
    };

    // `c[j] = sum_n a[n] b[n - j]` peaks at `j`; the reported convention is
    // `a[n] ≈ b[n + lag]`, so the lag is the negated index.
    Some(PhatResult {
        lag: -(peak_lag as f64 + raw_offset),
        peak: best_val,
        zscore: z,
    })
}

/// Parabolic interpolation over three scalar values.
fn parabolic_scalar(y0: f32, y1: f32, y2: f32) -> f64 {
    let (y0, y1, y2) = (y0 as f64, y1 as f64, y2 as f64);
    let denom = y0 - 2.0 * y1 + y2;
    if denom.abs() < 1e-12 {
        return 0.0;
    }
    (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5)
}
