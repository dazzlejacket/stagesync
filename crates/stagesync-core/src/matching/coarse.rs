//! Coarse global lock: find *where* in the reference a source recording lives.
//!
//! This stage must work across **hours**, not seconds. It does so by correlating several
//! long, non-quiet windows of the source against the whole reference, then requiring that
//! the resulting candidate offsets **agree**. A single window can be fooled by applause,
//! a quiet passage or a repeated section; a dozen windows that all point at the same
//! offset cannot be fooled by accident.

use serde::{Deserialize, Serialize};

use super::dsp::{find_peaks_by_ncc, pearson_correlate_masked};
use super::model::LinearModel;
use crate::features::Envelope;

/// Tunables for the coarse lock.
#[derive(Debug, Clone, Copy)]
pub struct CoarseConfig {
    /// Length of each correlation window, in seconds.
    pub window_secs: f64,
    /// Hop between candidate window positions, in seconds.
    pub hop_secs: f64,
    /// Decimation of the 100 Hz envelope used for the coarse pass.
    pub decimation: usize,
    /// Candidate peaks kept per window.
    pub max_candidates: usize,
    /// Minimum separation between candidate lags, in seconds.
    pub min_separation_secs: f64,
    /// Tolerance for grouping candidate lags into one cluster, in seconds.
    pub cluster_tolerance_secs: f64,
    /// Minimum fraction of non-quiet frames a window must contain.
    pub min_active_fraction: f64,
    /// Maximum number of windows to correlate.
    pub max_windows: usize,
    /// Minimum score of the whole-file correlation for it to anchor the lock.
    pub min_whole_file_score: f32,
    /// How far a window candidate may sit from the whole-file correlation and still
    /// count as agreeing with it, in seconds.
    pub whole_file_agreement_secs: f64,
}

impl Default for CoarseConfig {
    fn default() -> Self {
        Self {
            window_secs: 60.0,
            hop_secs: 300.0,
            decimation: 5,
            max_candidates: 3,
            min_separation_secs: 10.0,
            cluster_tolerance_secs: 2.0,
            min_active_fraction: 0.4,
            max_windows: 16,
            min_whole_file_score: 0.10,
            whole_file_agreement_secs: 10.0,
        }
    }
}

/// One window's contribution to the coarse lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseWindow {
    /// Centre of the window in source seconds.
    pub source_time: f64,
    /// Lag of this window's best candidate, in seconds.
    pub lag_secs: f64,
    /// Normalised correlation value of that candidate.
    pub score: f32,
    /// How many standard deviations the peak stood above the sidelobes.
    pub zscore: f32,
    /// Was this window accepted as non-quiet?
    pub accepted: bool,
}

/// A cluster of agreeing candidate offsets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseCandidate {
    /// Representative lag of the cluster, in seconds.
    pub lag_secs: f64,
    /// Number of distinct windows supporting this cluster.
    pub votes: usize,
    /// Mean correlation score of the supporting candidates.
    pub mean_score: f32,
    /// The per-window detail of the supporting candidates.
    pub windows: Vec<CoarseWindow>,
}

/// The outcome of the coarse lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoarseLock {
    /// All candidate clusters, best first.
    pub candidates: Vec<CoarseCandidate>,
    /// The winning cluster, if the lock succeeded.
    pub chosen: Option<CoarseCandidate>,
    /// A linear model fitted to the winning cluster's windows.
    pub model: Option<LinearModel>,
    /// Lag of the best whole-file correlation (an independent sanity check).
    pub full_range_lag_secs: Option<f64>,
    /// Matched-filter score of the best whole-file correlation.
    pub full_range_score: f32,
    /// Overlap-normalised Pearson correlation of the best whole-file peak.
    #[serde(default)]
    pub full_range_ncc: f32,
    /// `true` when the lock succeeded.
    pub locked: bool,
    /// Explanation when the lock did not succeed.
    pub reason: Option<String>,
}

/// Attempt the coarse global lock of `source` against `reference`.
pub fn coarse_lock(source: &Envelope, reference: &Envelope, config: &CoarseConfig) -> CoarseLock {
    let dec = config.decimation.max(1);
    let src_dec = decimate(&source.norm, dec);
    let ref_dec = decimate(&reference.norm, dec);
    let src_quiet_dec = decimate(&source.quiet, dec);
    let rate = source.rate_hz / dec as f64;

    if src_dec.len() < 10 || ref_dec.len() < 10 {
        return CoarseLock {
            candidates: Vec::new(),
            chosen: None,
            model: None,
            full_range_lag_secs: None,
            full_range_score: 0.0,
            full_range_ncc: 0.0,
            locked: false,
            reason: Some("source or reference is too short to correlate".into()),
        };
    }

    let default_window_len = ((config.window_secs * rate).round() as usize).max(8);
    let min_sub_window = ((4.0 * rate).round() as usize).max(8);

    // A recording shorter than the correlation window cannot be corroborated if the
    // single window equals the whole file. Split clips at least 8 s long into
    // overlapping sub-windows so multiple windows must agree (or disagree).
    let window_len = if src_dec.len() <= default_window_len && src_dec.len() >= min_sub_window * 2
    {
        (src_dec.len() / 2).max(min_sub_window)
    } else {
        default_window_len
    }
    .min(src_dec.len())
    .min(ref_dec.len());
    if window_len < 8 {
        return CoarseLock {
            candidates: Vec::new(),
            chosen: None,
            model: None,
            full_range_lag_secs: None,
            full_range_score: 0.0,
            full_range_ncc: 0.0,
            locked: false,
            reason: Some(
                "source or reference is shorter than the coarse correlation window".into(),
            ),
        };
    }

    // Window positions are spread over the whole recording so that the candidates can be
    // checked for *agreement*. A fixed hop would leave short recordings with a single
    // window, and a single window can never be corroborated.
    let usable = src_dec.len().saturating_sub(window_len);
    let by_hop = if config.hop_secs > 0.0 {
        (usable as f64 / (config.hop_secs * rate).max(1.0)).ceil() as usize
    } else {
        1
    };
    let n_windows = by_hop
        .clamp(3, config.max_windows.max(3))
        .min(usable + 1)
        .max(1);

    let mut all_positions: Vec<usize> = Vec::with_capacity(n_windows);
    if n_windows == 1 {
        all_positions.push(0);
    } else {
        let step = usable as f64 / (n_windows - 1) as f64;
        for i in 0..n_windows {
            all_positions.push((i as f64 * step).round() as usize);
        }
    }
    all_positions.dedup();

    // Prefer windows that contain real signal.
    let active: Vec<usize> = all_positions
        .iter()
        .copied()
        .filter(|&pos| {
            let active = src_quiet_dec[pos..pos + window_len]
                .iter()
                .filter(|q| !**q)
                .count() as f64
                / window_len as f64;
            active >= config.min_active_fraction
        })
        .collect();
    let total_windows = all_positions.len();
    let positions = if active.is_empty() {
        all_positions
    } else {
        active
    };

    let min_sep_samples = (config.min_separation_secs * rate).round() as i64;

    let mut raw: Vec<(f64, f32, f32, usize)> = Vec::new(); // lag s, score, zscore, window index
    let mut window_details: Vec<CoarseWindow> = Vec::new();

    for (window_index, &pos) in positions.iter().enumerate() {
        let query = &src_dec[pos..pos + window_len];
        // The silent part of a window carries no information; excluding it from both
        // sides of the comparison keeps a recorder gap from dragging the correlation
        // peak away from the true position.
        let mask = &src_quiet_dec[pos..pos + window_len];
        let corr = pearson_correlate_masked(query, &ref_dec, Some(mask));
        let peaks = find_peaks_by_ncc(&corr, config.max_candidates, min_sep_samples.max(1));
        let centre = (pos as f64 + window_len as f64 / 2.0) / rate;

        if peaks.is_empty() {
            window_details.push(CoarseWindow {
                source_time: centre,
                lag_secs: 0.0,
                score: 0.0,
                zscore: 0.0,
                accepted: false,
            });
            continue;
        }
        for (rank, p) in peaks.iter().enumerate() {
            // The correlation is between the window and the whole reference, so the
            // peak's lag is measured from the **start of the window**, not from the
            // start of the recording. The offset of the source within the reference is
            // the lag minus the window's own source position; without this correction
            // every window reports its position back as part of the offset, and no two
            // windows ever agree.
            let lag_s = (p.lag as f64 - pos as f64) / rate;
            raw.push((lag_s, p.value, p.zscore, window_index));
            if rank == 0 {
                window_details.push(CoarseWindow {
                    source_time: centre,
                    lag_secs: lag_s,
                    score: p.value,
                    zscore: p.zscore,
                    accepted: true,
                });
            }
        }
    }

    // Cluster the candidates by lag.
    let tolerance = config.cluster_tolerance_secs;
    let mut sorted = raw.clone();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut clusters: Vec<Vec<(f64, f32, f32, usize)>> = Vec::new();
    for cand in sorted {
        match clusters.last_mut() {
            Some(last) if (cand.0 - last[0].0).abs() <= tolerance => last.push(cand),
            _ => clusters.push(vec![cand]),
        }
    }

    let mut candidates: Vec<CoarseCandidate> = clusters
        .into_iter()
        .map(|cluster| {
            // The median, not the mean: one window that latched onto the wrong place
            // would otherwise drag the whole cluster's offset with it.
            let lag_secs = median(cluster.iter().map(|c| c.0).collect());
            let mean_score = cluster.iter().map(|c| c.1).sum::<f32>() / cluster.len() as f32;

            // One vote per distinct window, using that window's candidate nearest the
            // cluster representative.
            let mut windows: Vec<CoarseWindow> = Vec::new();
            let mut seen_windows: Vec<usize> = Vec::new();
            for (lag, score, z, widx) in &cluster {
                if seen_windows.contains(widx) {
                    continue;
                }
                seen_windows.push(*widx);
                let centre = window_details
                    .get(*widx)
                    .map(|d| d.source_time)
                    .unwrap_or(0.0);
                windows.push(CoarseWindow {
                    source_time: centre,
                    lag_secs: *lag,
                    score: *score,
                    zscore: *z,
                    accepted: true,
                });
            }
            CoarseCandidate {
                lag_secs,
                votes: windows.len(),
                mean_score,
                windows,
            }
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.votes.cmp(&a.votes).then_with(|| {
            b.mean_score
                .partial_cmp(&a.mean_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    // Whole-file correlation: the single most reliable measurement available, because
    // it uses every sample of both recordings at once. Individual windows can be fooled
    // by a passage that happens to sound like another one; the whole-file correlation
    // is what breaks that tie.
    //
    // It is only reliable while the recording has no hole in it. A recorder gap
    // contributes frames that carry no information, and correlating them against the
    // reference's music corrupts the peak: on a ninety-second source of which sixty
    // seconds are a gap, the unmasked whole-file correlation landed between ten and a
    // hundred and forty-four seconds away from the truth, and it overrode a window
    // consensus that had found the right offset. The gap is therefore excluded from
    // the comparison exactly as it is excluded from the window correlations — the
    // reference's energy is measured over precisely the same positions, so the two
    // sides are always compared over the same material.
    let (full_lag, full_score, full_ncc) =
        full_range_correlation(&src_dec, &ref_dec, &src_quiet_dec, rate);

    // Choose the candidate that the whole-file correlation agrees with. Without that
    // agreement, a candidate needs support from at least two windows before it is
    // trusted: a single window can match the wrong place in a long recording.
    //
    // Crucially, do NOT count whole-file correlation as independent corroboration when
    // there was only one coarse window spanning the entire source — that would merely
    // compare the same correlation against itself.
    let agreement = config.whole_file_agreement_secs;
    let mut chosen: Option<CoarseCandidate> = None;
    let mut whole_file_anchored = false;
    let whole_file_is_independent = total_windows >= 2 && window_len < src_dec.len();

    if whole_file_is_independent {
        if let Some(lag) = full_lag {
            if full_score >= config.min_whole_file_score {
                if let Some(c) = candidates
                    .iter()
                    .filter(|c| (c.lag_secs - lag).abs() <= agreement)
                    .min_by(|a, b| {
                        (a.lag_secs - lag)
                            .abs()
                            .partial_cmp(&(b.lag_secs - lag).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                {
                    chosen = Some(c.clone());
                    whole_file_anchored = true;
                }
            }
        }
    }
    if chosen.is_none() {
        chosen = candidates.first().cloned();
    }

    let mut locked = false;
    let mut model = None;
    let mut reason = None;

    match &chosen {
        Some(c) if whole_file_anchored || c.votes >= 2 => {
            locked = true;
            // The coarse stage answers **where** this recording lives, not **how fast its
            // clock ran**. Its measurements are the lags of a handful of 60 s windows read
            // off a 20 Hz decimated envelope, so they are quantised at 50 ms. Fitting a
            // *rate* to them turns a single 1 s lag error into a 4 % rate error — which
            // then displaces every fine probe and makes the whole alignment fail. The
            // ppm-level drift is measured by the fine stage instead, where hundreds of
            // probes are available.
            //
            // The offset comes from the most reliable measurement there is: the
            // whole-file correlation, which uses every sample of both recordings at once.
            // When that did not anchor the lock, the cluster's own consensus is used.
            let offset = match full_lag {
                Some(lag) if whole_file_anchored => lag,
                _ => c.lag_secs,
            };
            model = Some(LinearModel::new(1.0, offset));
        }
        Some(c) => {
            reason = Some(format!(
                "best offset candidate was supported by only {} window(s) and the whole-file \
                 correlation did not confirm it; refusing to lock on a single ambiguous match",
                c.votes
            ));
        }
        None => {
            reason = Some("no usable correlation peak was found".into());
        }
    }

    CoarseLock {
        candidates,
        chosen,
        model,
        full_range_lag_secs: full_lag,
        full_range_score: full_score,
        full_range_ncc: full_ncc,
        locked,
        reason,
    }
}

/// Correlate the whole source envelope against the whole reference envelope.
///
/// The source's silent frames are excluded from the comparison, so that a recorder
/// gap cannot drag the peak away from the true correspondence. See the call site for
/// why this measurement has to be masked as well.
fn full_range_correlation(
    src_dec: &[f32],
    ref_dec: &[f32],
    src_quiet_dec: &[bool],
    rate: f64,
) -> (Option<f64>, f32, f32) {
    let corr = pearson_correlate_masked(src_dec, ref_dec, Some(src_quiet_dec));
    let peaks = find_peaks_by_ncc(&corr, 1, 1);
    match peaks.first() {
        Some(p) => (Some(p.lag as f64 / rate), p.value, p.ncc),
        None => (None, 0.0, 0.0),
    }
}

/// Take every `n`-th element of a slice.
fn decimate<T: Copy>(values: &[T], n: usize) -> Vec<T> {
    if n <= 1 {
        return values.to_vec();
    }
    values.iter().step_by(n).copied().collect()
}

/// The median of a set of values.
///
/// Used instead of the mean wherever a single wrong measurement would otherwise drag
/// the result: a mean of lags lets one window that latched onto the wrong place move
/// the consensus.
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values[values.len() / 2]
}
