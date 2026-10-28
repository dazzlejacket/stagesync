//! Robust regression for the drift model.
//!
//! Live music breaks naive least squares: repeated choruses produce clusters of equally
//! plausible matches, and a single missed probe can drag a fit. Every estimator here is
//! therefore built to tolerate a sizeable minority of wrong points.

use super::model::LinearModel;

/// A fitted line plus the diagnostics needed to judge it.
#[derive(Debug, Clone)]
pub struct LinearFit {
    /// Fitted slope (`reference_time` per `source_time`).
    pub slope: f64,
    /// Fitted intercept.
    pub intercept: f64,
    /// Residuals of the input points, in the input order.
    pub residuals: Vec<f64>,
    /// Which input points survived the outlier rejection.
    pub inliers: Vec<bool>,
}

impl LinearFit {
    /// The fit expressed as a [`LinearModel`].
    pub fn model(&self) -> LinearModel {
        LinearModel::new(self.slope, self.intercept)
    }

    /// Root-mean-square residual of the inliers.
    pub fn rms_residual(&self) -> f64 {
        let vals: Vec<f64> = self
            .residuals
            .iter()
            .zip(self.inliers.iter())
            .filter(|(_, ok)| **ok)
            .map(|(r, _)| *r)
            .collect();
        if vals.is_empty() {
            return 0.0;
        }
        (vals.iter().map(|v| v * v).sum::<f64>() / vals.len() as f64).sqrt()
    }

    /// Median absolute residual of the inliers.
    pub fn mad_residual(&self) -> f64 {
        let mut vals: Vec<f64> = self
            .residuals
            .iter()
            .zip(self.inliers.iter())
            .filter(|(_, ok)| **ok)
            .map(|(r, _)| r.abs())
            .collect();
        if vals.is_empty() {
            return 0.0;
        }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        vals[vals.len() / 2]
    }
}

/// TheilÃ¢â‚¬â€œSen estimator: the median of all pairwise slopes.
///
/// Its breakdown point is ~29 %, so it survives a substantial fraction of wrong matches
/// without needing any tuning.
pub fn theil_sen(points: &[(f64, f64)]) -> Option<LinearFit> {
    if points.len() < 2 {
        return None;
    }
    let mut slopes = Vec::with_capacity(points.len() * (points.len() - 1) / 2);
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let dx = points[j].0 - points[i].0;
            if dx.abs() > 1e-12 {
                slopes.push((points[j].1 - points[i].1) / dx);
            }
        }
    }
    if slopes.is_empty() {
        return None;
    }
    slopes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let slope = slopes[slopes.len() / 2];

    let mut intercepts: Vec<f64> = points.iter().map(|(x, y)| *y - slope * *x).collect();
    intercepts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let intercept = intercepts[intercepts.len() / 2];

    Some(finalise(slope, intercept, points, points.len()))
}

/// Least-squares line through the given points.
pub fn least_squares(points: &[(f64, f64)]) -> Option<LinearFit> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for (x, y) in points {
        sxy += (x - mean_x) * (y - mean_y);
        sxx += (x - mean_x) * (x - mean_x);
    }
    if sxx.abs() < 1e-18 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    Some(finalise(slope, intercept, points, points.len()))
}

/// RANSAC: fit the line supported by the largest set of mutually consistent points.
///
/// Deterministic: the sampling uses a fixed-seed LCG, so repeated runs produce identical
/// results.
pub fn ransac(
    points: &[(f64, f64)],
    threshold: f64,
    iterations: usize,
    seed: u64,
) -> Option<LinearFit> {
    if points.len() < 3 {
        return theil_sen(points);
    }
    let mut rng = Lcg::new(seed);
    let mut best: Option<(usize, f64, f64)> = None;

    for _ in 0..iterations {
        let i = (rng.next_u64() as usize) % points.len();
        let j = (rng.next_u64() as usize) % points.len();
        if i == j {
            continue;
        }
        let (x1, y1) = points[i];
        let (x2, y2) = points[j];
        let dx = x2 - x1;
        if dx.abs() < 1e-12 {
            continue;
        }
        let slope = (y2 - y1) / dx;
        let intercept = y1 - slope * x1;

        let inliers = points
            .iter()
            .filter(|(x, y)| (slope * x + intercept - y).abs() <= threshold)
            .count();
        if best.as_ref().is_none_or(|(c, _, _)| inliers > *c) {
            best = Some((inliers, slope, intercept));
        }
    }

    let (inliers, slope, intercept) = best?;
    // Refit by least squares over the inlier set.
    let subset: Vec<(f64, f64)> = points
        .iter()
        .filter(|(x, y)| (slope * x + intercept - y).abs() <= threshold)
        .copied()
        .collect();
    let refined = if subset.len() >= 2 {
        least_squares(&subset)
    } else {
        None
    };
    let (slope, intercept) = match refined {
        Some(f) => (f.slope, f.intercept),
        None => (slope, intercept),
    };
    let _ = inliers;
    Some(finalise(slope, intercept, points, points.len()))
}

/// Fit a line and iteratively reject outliers (points more than `k` MADs from the fit).
///
/// This is the workhorse: TheilÃ¢â‚¬â€œSen provides a robust starting slope, and this pass
/// removes the points that do not belong to the dominant correspondence.
pub fn robust_fit(points: &[(f64, f64)], k: f64, max_iterations: usize) -> Option<LinearFit> {
    let initial = theil_sen(points)?;
    let mut fit = initial;
    for _ in 0..max_iterations {
        let mad = fit.mad_residual().max(1e-9);
        let mut inliers = vec![true; points.len()];
        let mut changed = false;
        for (i, r) in fit.residuals.iter().enumerate() {
            if r.abs() > k * mad {
                inliers[i] = false;
                changed = true;
            }
        }
        let kept: Vec<(f64, f64)> = points
            .iter()
            .zip(inliers.iter())
            .filter(|(_, ok)| **ok)
            .map(|((x, y), _)| (*x, *y))
            .collect();
        if kept.len() < 2 || !changed {
            break;
        }
        if let Some(next) = least_squares(&kept) {
            fit = finalise(next.slope, next.intercept, points, kept.len());
        } else {
            break;
        }
    }
    // Recompute the inlier mask against the final fit.
    let mad = fit.mad_residual().max(1e-9);
    let inliers: Vec<bool> = fit.residuals.iter().map(|r| r.abs() <= k * mad).collect();
    Some(LinearFit {
        slope: fit.slope,
        intercept: fit.intercept,
        residuals: fit.residuals,
        inliers,
    })
}

/// Fit a line to points that carry per-point weights, using weighted least squares.
pub fn weighted_least_squares(points: &[(f64, f64)], weights: &[f64]) -> Option<LinearFit> {
    if points.len() < 2 || points.len() != weights.len() {
        return None;
    }
    let mut sw = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for (i, (x, y)) in points.iter().enumerate() {
        let w = weights[i].max(0.0);
        sw += w;
        sx += w * x;
        sy += w * y;
    }
    if sw <= 0.0 {
        return None;
    }
    let mean_x = sx / sw;
    let mean_y = sy / sw;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for (i, (x, y)) in points.iter().enumerate() {
        let w = weights[i].max(0.0);
        sxy += w * (x - mean_x) * (y - mean_y);
        sxx += w * (x - mean_x) * (x - mean_x);
    }
    if sxx.abs() < 1e-18 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    Some(finalise(slope, intercept, points, points.len()))
}

/// Compute residuals for a candidate slope/intercept.
pub fn finalise(
    slope: f64,
    intercept: f64,
    points: &[(f64, f64)],
    inlier_count: usize,
) -> LinearFit {
    let residuals: Vec<f64> = points
        .iter()
        .map(|(x, y)| slope * x + intercept - y)
        .collect();
    let mut inliers = vec![true; points.len()];
    // Mark the worst `points.len() - inlier_count` points as outliers.
    if inlier_count < points.len() {
        let mut order: Vec<usize> = (0..points.len()).collect();
        order.sort_by(|&a, &b| {
            residuals[b]
                .abs()
                .partial_cmp(&residuals[a].abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for &i in &order[..points.len() - inlier_count] {
            inliers[i] = false;
        }
    }
    LinearFit {
        slope,
        intercept,
        residuals,
        inliers,
    }
}

/// A small deterministic linear congruential generator (Numerical Recipes constants).
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(6364136223846793005).wrapping_add(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}
