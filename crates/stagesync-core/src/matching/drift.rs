//! How well the drift is actually determined — and when to stop guessing.
//!
//! A drift is a *slope*, and a slope fitted through points that each carry a
//! millisecond of noise is only good to about `noise / sqrt(Sxx)`, where `Sxx` is the
//! spread of the source times the fit went through. On a short recording the drift
//! signal is smaller than that noise, so the fitted slope is a number without a
//! measurement behind it. Reporting it as a measurement is how a synchroniser ends up
//! confidently wrong.
//!
//! This module therefore does three things:
//!
//! * it computes the **standard error of a slope**, so a drift figure always travels
//!   with the precision that produced it;
//! * it decides, from the evidence, whether the drift may be applied at all;
//! * when it may not, it falls back to an **offset-only** correction and says so,
//!   rather than applying a number it cannot defend.
//!
//! Two independent estimators feed the decision: the RMS-envelope matcher and the
//! raw-waveform correlation. Where they disagree, the disagreement is reported rather
//! than averaged away.

use serde::{Deserialize, Serialize};

use super::model::LinearModel;

/// A line fit that carries the uncertainty of its slope.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SlopeFit {
    /// Fitted slope: seconds of reference per second of source.
    pub slope: f64,
    /// Fitted intercept, in seconds.
    pub intercept: f64,
    /// Standard error of the slope, in the same units as the slope.
    pub slope_se: f64,
    /// Root-mean-square residual of the fit, in seconds.
    pub residual_rms: f64,
    /// Number of points the fit used.
    pub points: usize,
    /// The spread of the source times, `sqrt(sum((t - t_bar)^2))`, in seconds.
    pub time_span: f64,
}

impl SlopeFit {
    /// The standard error of the slope, in parts per million.
    pub fn slope_se_ppm(&self) -> f64 {
        self.slope_se * 1_000_000.0
    }

    /// The drift the fit describes, in parts per million.
    pub fn drift_ppm(&self) -> f64 {
        (self.slope - 1.0) * 1_000_000.0
    }

    /// How many times the drift exceeds its own uncertainty.
    ///
    /// The drift is the slope's departure from unity, not the slope itself: a
    /// clock that runs at exactly the reference's rate has a slope of one and no
    /// drift at all, so the comparison is against one and not against zero.
    ///
    /// A value below one means the drift is not distinguishable from zero; the sign of
    /// the drift would then be a property of the noise.
    pub fn signal_to_noise(&self) -> f64 {
        if self.slope_se <= 0.0 {
            f64::INFINITY
        } else {
            (self.slope - 1.0).abs() / self.slope_se
        }
    }

    /// The fit expressed as a [`LinearModel`].
    pub fn model(&self) -> LinearModel {
        LinearModel::new(self.slope, self.intercept)
    }
}

/// Fit a line through the points and report the standard error of its slope.
///
/// The standard error is the classical one for ordinary least squares,
/// `sigma / sqrt(Sxx)`, with `sigma` estimated from the residuals. It assumes the
/// residuals are independent; where they are not, the true error is larger, so this is
/// a floor and not a guarantee.
pub fn fit_with_uncertainty(points: &[(f64, f64)]) -> Option<SlopeFit> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for (x, y) in points {
        sxy += (x - mean_x) * (y - mean_y);
        sxx += (x - mean_x) * (x - mean_x);
    }
    if sxx <= 1e-18 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;

    let residuals: Vec<f64> = points
        .iter()
        .map(|(x, y)| y - (slope * x + intercept))
        .collect();
    let sum_sq = residuals.iter().map(|r| r * r).sum::<f64>();
    let residual_rms = (sum_sq / n as f64).sqrt();

    // Two degrees of freedom are spent on the slope and the intercept.
    let dof = (n - 2) as f64;
    let slope_se = if dof > 0.0 {
        (sum_sq / dof / sxx).sqrt()
    } else {
        f64::INFINITY
    };

    Some(SlopeFit {
        slope,
        intercept,
        slope_se,
        residual_rms,
        points: n,
        time_span: sxx.sqrt(),
    })
}

/// Fit the offset alone, holding the scale at one.
///
/// This is the offset-only correction: the material is moved in time but its rate is
/// left alone. It is the honest fallback when the drift cannot be established.
pub fn fit_offset_only(points: &[(f64, f64)]) -> Option<LinearModel> {
    if points.is_empty() {
        return None;
    }
    let mean = points.iter().map(|(x, y)| y - x).sum::<f64>() / points.len() as f64;
    Some(LinearModel::new(1.0, mean))
}

/// Which estimator produced a drift figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftMethod {
    /// The RMS-envelope matcher.
    Envelope,
    /// The raw-waveform correlation.
    Waveform,
    /// No drift: only the offset was corrected.
    OffsetOnly,
}

impl DriftMethod {
    /// A short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            DriftMethod::Envelope => "envelope",
            DriftMethod::Waveform => "waveform",
            DriftMethod::OffsetOnly => "offset only",
        }
    }
}

/// Whether the drift is established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftStatus {
    /// The drift was measured and its uncertainty is small enough to trust it.
    Established,
    /// The drift could not be established; only the offset is corrected.
    OffsetOnly,
}

/// The figures one estimator produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodFigures {
    /// Which estimator these are.
    pub method: DriftMethod,
    /// The drift, in parts per million.
    pub drift_ppm: f64,
    /// The time scale the fit describes.
    ///
    /// This is carried separately from `drift_ppm` because converting the scale back
    /// from parts per million is a lossy round trip, and the model that gets applied
    /// should be exactly the model that was measured.
    pub scale: f64,
    /// The standard error of that drift, in parts per million.
    pub slope_se_ppm: f64,
    /// The root-mean-square residual of the fit, in milliseconds.
    pub residual_rms_ms: f64,
    /// How many measurements the fit used.
    pub points_used: usize,
    /// The reference time of source time zero implied by the fit, in seconds.
    pub offset_seconds: f64,
    /// The mean normalised correlation of the raw waveform, when the estimator has one.
    ///
    /// `None` for the envelope estimator, which never looks at the waveform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_correlation: Option<f64>,
}

impl MethodFigures {
    /// How many times the drift exceeds its own uncertainty.
    pub fn signal_to_noise(&self) -> f64 {
        if self.slope_se_ppm <= 0.0 {
            f64::INFINITY
        } else {
            self.drift_ppm.abs() / self.slope_se_ppm
        }
    }
}

/// The thresholds that decide whether a drift may be applied.
#[derive(Debug, Clone, Copy)]
pub struct DriftOptions {
    /// The drift must exceed its own uncertainty by this factor to be applied.
    pub min_signal_to_noise: f64,
    /// The smallest number of measurements a fit may rest on.
    pub min_points: usize,
    /// The largest residual RMS a waveform fit may have and still be trusted, in ms.
    pub max_waveform_residual_ms: f64,
    /// The smallest mean raw-waveform correlation a waveform fit may have.
    ///
    /// Two *different* microphones recording the same performance have uncorrelated
    /// waveforms but strongly correlated envelopes. This threshold is what tells the
    /// two cases apart, so it is deliberately conservative.
    pub min_waveform_correlation: f64,
    /// The disagreement between the two methods that is still agreement, in ppm.
    pub agreement_tolerance_ppm: f64,
}

impl Default for DriftOptions {
    fn default() -> Self {
        Self {
            min_signal_to_noise: 3.0,
            min_points: 3,
            max_waveform_residual_ms: 5.0,
            min_waveform_correlation: 0.30,
            agreement_tolerance_ppm: 2.0,
        }
    }
}

/// The verdict on one source's drift, and the model that follows from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAssessment {
    /// Whether the drift is established.
    pub status: DriftStatus,
    /// Which estimator the applied model came from.
    pub method: DriftMethod,
    /// The model that will be used to correct the source.
    pub model: LinearModel,
    /// The drift that will be applied, in parts per million.
    pub drift_ppm: f64,
    /// The standard error of the applied drift, in parts per million.
    pub slope_se_ppm: f64,
    /// The root-mean-square residual of the applied fit, in milliseconds.
    pub residual_rms_ms: f64,
    /// How many measurements the applied fit used.
    pub points_used: usize,
    /// The envelope estimator's figures, for comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<MethodFigures>,
    /// The waveform estimator's figures, for comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform: Option<MethodFigures>,
    /// Why the decision went the way it did.
    pub notes: Vec<String>,
}

impl DriftAssessment {
    /// The drift the model applies, in parts per million.
    pub fn drift_ppm(&self) -> f64 {
        self.model.drift_ppm()
    }
}

/// Decide which drift to apply from what the two estimators measured.
///
/// The rules, in order:
///
/// * a waveform measurement that is well determined wins, because it measures the
///   position on the *raw signal* and therefore has far less noise than the envelope;
/// * a well-determined envelope measurement is used when no usable waveform
///   measurement exists — this is the case for two independent microphones, where the
///   raw waveforms are uncorrelated by construction;
/// * when neither is well determined, the drift is **not** applied: the correction
///   falls back to the offset alone and the reason is recorded.
pub fn assess(
    envelope: Option<MethodFigures>,
    waveform: Option<MethodFigures>,
    waveform_attempted: bool,
    fallback_model: LinearModel,
    options: &DriftOptions,
) -> DriftAssessment {
    let mut notes = Vec::new();

    let envelope_usable = envelope.as_ref().is_some_and(|f| {
        f.points_used >= options.min_points
            && f.slope_se_ppm > 0.0
            && f.signal_to_noise() >= options.min_signal_to_noise
    });
    let waveform_usable = waveform.as_ref().is_some_and(|f| {
        f.points_used >= options.min_points
            && f.slope_se_ppm > 0.0
            && f.signal_to_noise() >= options.min_signal_to_noise
            && f.residual_rms_ms <= options.max_waveform_residual_ms
            && f.mean_correlation
                .is_some_and(|c| c >= options.min_waveform_correlation)
    });

    if let Some(f) = &envelope {
        if !envelope_usable {
            notes.push(format!(
                "the envelope estimate ({:+.2} ppm, uncertainty {:.2} ppm) is not precise enough to trust \
                 (it needs to exceed its own uncertainty by a factor of {:.1})",
                f.drift_ppm,
                f.slope_se_ppm,
                options.min_signal_to_noise
            ));
        }
    }
    if let Some(f) = &waveform {
        if !waveform_usable {
            let reason = if f.points_used < options.min_points {
                format!("only {} usable probes", f.points_used)
            } else if f
                .mean_correlation
                .is_some_and(|c| c < options.min_waveform_correlation)
            {
                format!(
                    "the raw waveforms are not the same signal (mean correlation {:.2})",
                    f.mean_correlation.unwrap_or(0.0)
                )
            } else if f.residual_rms_ms > options.max_waveform_residual_ms {
                format!("the probes disagree by {:.2} ms", f.residual_rms_ms)
            } else {
                format!(
                    "the drift ({:+.2} ppm) does not exceed its own uncertainty ({:.2} ppm)",
                    f.drift_ppm, f.slope_se_ppm
                )
            };
            notes.push(format!(
                "no usable waveform-domain drift measurement ({reason})"
            ));
        }
    }

    let chosen: (DriftMethod, MethodFigures) = match (envelope.clone(), waveform.clone()) {
        (Some(env), Some(wave)) if waveform_usable => {
            let difference = (env.drift_ppm - wave.drift_ppm).abs();
            let tolerance = options
                .agreement_tolerance_ppm
                .max(3.0 * env.slope_se_ppm)
                .max(3.0 * wave.slope_se_ppm);
            if difference > tolerance {
                notes.push(format!(
                    "the envelope estimate ({:+.2} ppm) and the waveform estimate ({:+.2} ppm) differ by \
                     {difference:.2} ppm, which is more than three times the envelope's own uncertainty of \
                     {:.2} ppm; the waveform estimate was used because it is the more precise measurement",
                    env.drift_ppm, wave.drift_ppm, env.slope_se_ppm
                ));
            } else {
                notes.push(format!(
                    "the envelope estimate ({:+.2} ppm) and the waveform estimate ({:+.2} ppm) differ by \
                     {difference:.2} ppm, which is within three times the envelope's own uncertainty of \
                     {:.2} ppm; the waveform estimate was used because it is the more precise measurement",
                    env.drift_ppm, wave.drift_ppm, env.slope_se_ppm
                ));
            }
            (DriftMethod::Waveform, wave)
        }
        (Some(env), Some(_wave)) if envelope_usable => {
            notes.push(
                "the waveform-domain measurement could not be trusted, so the envelope estimate was used"
                    .to_string(),
            );
            (DriftMethod::Envelope, env)
        }
        (Some(env), None) if envelope_usable => {
            notes.push(if waveform_attempted {
                "the waveform-domain measurement was rejected, so the envelope estimate was used"
                    .to_string()
            } else {
                "no waveform-domain measurement was available, so the envelope estimate was used"
                    .to_string()
            });
            (DriftMethod::Envelope, env)
        }
        (None, Some(wave)) if waveform_usable => {
            notes.push(
                "the envelope estimate was not available, so the waveform estimate was used"
                    .to_string(),
            );
            (DriftMethod::Waveform, wave)
        }
        (Some(_), Some(_)) | (Some(_), None) => {
            notes.push(
                "neither the envelope nor the waveform estimate is precise enough to establish the drift; \
                 only the offset was corrected"
                    .to_string(),
            );
            return offset_only(fallback_model, envelope, waveform, notes);
        }
        (None, Some(_)) => {
            notes.push(
                "neither the envelope nor the waveform estimate is precise enough to establish the drift; \
                 only the offset was corrected"
                    .to_string(),
            );
            return offset_only(fallback_model, envelope, waveform, notes);
        }
        (None, None) => {
            notes.push(
                "no drift measurement was available; only the offset was corrected".to_string(),
            );
            return offset_only(fallback_model, envelope, waveform, notes);
        }
    };

    DriftAssessment {
        status: DriftStatus::Established,
        method: chosen.0,
        model: LinearModel::new(chosen.1.scale, chosen.1.intercept_seconds()),
        drift_ppm: chosen.1.drift_ppm,
        slope_se_ppm: chosen.1.slope_se_ppm,
        residual_rms_ms: chosen.1.residual_rms_ms,
        points_used: chosen.1.points_used,
        envelope,
        waveform,
        notes,
    }
}

/// Build an offset-only assessment.
fn offset_only(
    fallback_model: LinearModel,
    envelope: Option<MethodFigures>,
    waveform: Option<MethodFigures>,
    notes: Vec<String>,
) -> DriftAssessment {
    DriftAssessment {
        status: DriftStatus::OffsetOnly,
        method: DriftMethod::OffsetOnly,
        model: LinearModel::new(1.0, fallback_model.offset),
        drift_ppm: 0.0,
        // No drift is applied, so the applied drift has no uncertainty of its own. The
        // uncertainty of the *rejected* estimates is in `envelope` and `waveform`.
        slope_se_ppm: 0.0,
        residual_rms_ms: 0.0,
        points_used: 0,
        envelope,
        waveform,
        notes,
    }
}

impl MethodFigures {
    /// The reference time of source time zero implied by this fit.
    pub fn intercept_seconds(&self) -> f64 {
        self.offset_seconds
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn figures(drift_ppm: f64, se_ppm: f64, points: usize) -> MethodFigures {
        MethodFigures {
            method: DriftMethod::Envelope,
            drift_ppm,
            scale: 1.0 + drift_ppm / 1_000_000.0,
            slope_se_ppm: se_ppm,
            residual_rms_ms: 1.0,
            points_used: points,
            offset_seconds: 0.25,
            mean_correlation: None,
        }
    }

    #[test]
    fn the_standard_error_of_a_slope_shrinks_as_the_points_spread() {
        let narrow: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let x = i as f64;
                (x, 1.000_000_05 * x + 0.1)
            })
            .collect();
        let wide: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let x = i as f64 * 100.0;
                (x, 1.000_000_05 * x + 0.1)
            })
            .collect();

        let a = fit_with_uncertainty(&narrow).expect("the narrow fit");
        let b = fit_with_uncertainty(&wide).expect("the wide fit");

        assert!(
            b.slope_se < a.slope_se,
            "a wider spread of points must give a smaller standard error"
        );
    }

    #[test]
    fn a_drift_that_does_not_exceed_its_uncertainty_is_not_applied() {
        let assessment = assess(
            Some(figures(-47.7, 40.0, 12)),
            None,
            true,
            LinearModel::new(1.0, 0.25),
            &DriftOptions::default(),
        );

        assert_eq!(assessment.status, DriftStatus::OffsetOnly);
        assert_eq!(assessment.method, DriftMethod::OffsetOnly);
        assert_eq!(assessment.model.scale, 1.0, "the rate was changed anyway");
        assert_eq!(assessment.model.offset, 0.25);
    }

    #[test]
    fn a_precise_waveform_measurement_overrides_a_noisy_envelope_one() {
        let mut wave = figures(34.8, 0.2, 18);
        wave.method = DriftMethod::Waveform;
        wave.mean_correlation = Some(0.72);

        let assessment = assess(
            Some(figures(-47.7, 40.0, 12)),
            Some(wave),
            true,
            LinearModel::new(1.0, 0.25),
            &DriftOptions::default(),
        );

        assert_eq!(assessment.status, DriftStatus::Established);
        assert_eq!(assessment.method, DriftMethod::Waveform);
        assert!((assessment.drift_ppm - 34.8).abs() < 1e-6);
    }

    #[test]
    fn an_unrelated_waveform_measurement_is_rejected() {
        let mut wave = figures(999.0, 0.2, 18);
        wave.method = DriftMethod::Waveform;
        wave.mean_correlation = Some(0.05);

        let assessment = assess(
            Some(figures(16.5, 1.0, 300)),
            Some(wave),
            true,
            LinearModel::new(1.0, 0.25),
            &DriftOptions::default(),
        );

        assert_eq!(assessment.method, DriftMethod::Envelope);
        assert!((assessment.drift_ppm - 16.5).abs() < 1e-6);
    }

    #[test]
    fn the_offset_only_fit_leaves_the_rate_alone() {
        let points = vec![(0.0, 0.5), (10.0, 10.4), (20.0, 20.3)];
        let model = fit_offset_only(&points).expect("the fit");
        assert_eq!(model.scale, 1.0);
        assert!((model.offset - 0.4).abs() < 1e-9);
    }
}
