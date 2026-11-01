//! End-to-end calibration.
//!
//! Fitting a line to measured correspondences and applying it are two different things.
//! A measurement can be *biased*: periodic music makes envelope correlation pick the
//! wrong peak, and a systematic bias in the estimator is indistinguishable from real
//! drift at the estimator's own resolution.
//!
//! The calibration loop closes that gap without asking the estimator to confirm itself:
//!
//! 1. apply the current model to the source (a pure time warp),
//! 2. **measure the warped result against the reference again**,
//! 3. fit the residual relation,
//! 4. fold the residual back into the model.
//!
//! Step 2 measures the *corrected* material, so a bias in the original estimator shows up
//! as a residual rather than being baked into the answer. The result is still an
//! envelope-domain measurement, so it is **not** accepted as final proof: the holdout
//! verifier in [`crate::verify`] measures the rendered waveform independently, and the
//! two are reported separately.
//!
//! The calibration is deliberately *bounded*. If the correction it proposes is larger than
//! a sane threshold, it is reported as a disagreement instead of being applied.

use serde::{Deserialize, Serialize};

use crate::features::Envelope;
use crate::matching::fine::{fine_match, FineOptions};
use crate::matching::model::LinearModel;
use crate::resample::{StreamingResampler, DEFAULT_PHASES, DEFAULT_TAPS};

/// Configuration for the calibration loop.
#[derive(Debug, Clone, Copy)]
pub struct CalibrationConfig {
    /// Maximum correction the loop is allowed to apply, in ppm of scale.
    pub max_scale_correction_ppm: f64,
    /// Maximum offset correction the loop is allowed to apply, in seconds.
    pub max_offset_correction_secs: f64,
    /// Resampler taps used to warp the envelope.
    pub taps: usize,
    /// Resampler phases used to warp the envelope.
    pub phases: usize,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            max_scale_correction_ppm: 100.0,
            max_offset_correction_secs: 2.0,
            taps: DEFAULT_TAPS,
            phases: DEFAULT_PHASES,
        }
    }
}

/// The outcome of the calibration loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    /// The model before calibration (fitted directly from the fine matches).
    pub direct_model: LinearModel,
    /// The model after calibration.
    pub calibrated_model: LinearModel,
    /// Scale of the calibrated material relative to the direct model (`1.0` = no change).
    pub measured_scale_ratio: f64,
    /// Offset of the calibrated material relative to the direct model, in seconds.
    pub measured_offset_delta: f64,
    /// Residual statistics after calibration, in milliseconds.
    pub residual_p50_ms: f64,
    pub residual_p95_ms: f64,
    pub residual_max_ms: f64,
    /// `true` when the correction was applied.
    pub applied: bool,
    /// Why the correction was not applied, when it was not.
    pub reason: Option<String>,
    /// Warnings raised during calibration.
    pub warnings: Vec<String>,
}

/// Run the calibration loop.
///
/// `source` and `reference` are envelopes of the *uncorrected* material.
pub fn calibrate(
    source: &Envelope,
    reference: &Envelope,
    direct: &LinearModel,
    fine_options: &FineOptions,
    config: &CalibrationConfig,
) -> CalibrationResult {
    let mut warnings = Vec::new();

    // 1. Warp the source envelope with the current model: c(u) = s(u / scale).
    let warped = warp_envelope(source, direct.scale, config);
    if warped.len() < 16 {
        return CalibrationResult {
            direct_model: *direct,
            calibrated_model: *direct,
            measured_scale_ratio: 1.0,
            measured_offset_delta: 0.0,
            residual_p50_ms: 0.0,
            residual_p95_ms: 0.0,
            residual_max_ms: 0.0,
            applied: false,
            reason: Some("source is too short to calibrate".into()),
            warnings,
        };
    }

    // 2. Measure the warped material against the reference. The warped material sits on
    //    the corrected time axis; the model that describes it is `scale = 1, offset = b`.
    let placement = LinearModel::new(1.0, direct.offset);
    let fine = fine_match(&warped, reference, &placement, fine_options);

    let Some(measured) = fine.model else {
        return CalibrationResult {
            direct_model: *direct,
            calibrated_model: *direct,
            measured_scale_ratio: 1.0,
            measured_offset_delta: 0.0,
            residual_p50_ms: fine.residual_p50_ms,
            residual_p95_ms: fine.residual_p95_ms,
            residual_max_ms: fine.residual_max_ms,
            applied: false,
            reason: Some("calibration pass could not establish a correspondence".into()),
            warnings,
        };
    };

    // 3. Fold back. The warped material at corrected time u holds source content from
    //    source time u / scale, so reference_time = measured.scale * (u / scale) + offset.
    let measured_scale_ratio = measured.scale;
    let measured_offset_delta = measured.offset - direct.offset;

    let new_scale = direct.scale * measured_scale_ratio;
    let new_offset = measured.offset;
    let calibrated = LinearModel::new(new_scale, new_offset);

    let scale_delta_ppm = (measured_scale_ratio - 1.0) * 1_000_000.0;
    let offset_delta = measured_offset_delta;

    // 4. Bound the correction. A large disagreement is a *finding*, not something to apply.
    let mut applied = true;
    let mut reason = None;
    if scale_delta_ppm.abs() > config.max_scale_correction_ppm {
        applied = false;
        reason = Some(format!(
            "calibration proposes a scale change of {scale_delta_ppm:.2} ppm, above the \
             {:.0} ppm bound; the direct model is kept and the disagreement is reported",
            config.max_scale_correction_ppm
        ));
        warnings.push(reason.clone().unwrap_or_default());
    }
    if offset_delta.abs() > config.max_offset_correction_secs {
        applied = false;
        reason = Some(format!(
            "calibration proposes an offset change of {offset_delta:.4} s, above the {:.2} s bound; \
             the direct model is kept and the disagreement is reported",
            config.max_offset_correction_secs
        ));
        warnings.push(reason.clone().unwrap_or_default());
    }
    if fine.measured < 5 {
        applied = false;
        reason = Some(format!(
            "calibration had only {} usable probes; the direct model is kept",
            fine.measured
        ));
        warnings.push(reason.clone().unwrap_or_default());
    }

    let final_model = if applied { calibrated } else { *direct };

    CalibrationResult {
        direct_model: *direct,
        calibrated_model: final_model,
        measured_scale_ratio,
        measured_offset_delta,
        residual_p50_ms: fine.residual_p50_ms,
        residual_p95_ms: fine.residual_p95_ms,
        residual_max_ms: fine.residual_max_ms,
        applied,
        reason,
        warnings,
    }
}

/// Warp an envelope by a time scale: `out[n] = in[n / scale]`.
pub fn warp_envelope(env: &Envelope, scale: f64, config: &CalibrationConfig) -> Envelope {
    let alpha = 1.0 / scale;
    let mut db: Vec<f32> = Vec::new();
    let mut norm: Vec<f32> = Vec::new();

    // Resample both series with the same map.
    let mut db_res = StreamingResampler::new(alpha, 0.0, config.taps, config.phases);
    let mut norm_res = StreamingResampler::new(alpha, 0.0, config.taps, config.phases);
    let block = 1 << 16;
    let mut offset = 0usize;
    while offset < env.len() {
        let end = (offset + block).min(env.len());
        let d = db_res.push(&env.db[offset..end]);
        let n = norm_res.push(&env.norm[offset..end]);
        db.extend_from_slice(&d);
        norm.extend_from_slice(&n);
        offset = end;
    }
    db.extend_from_slice(&db_res.flush());
    norm.extend_from_slice(&norm_res.flush());

    // The quiet flags follow the warped timeline, and they have to follow it in
    // the **same** direction as the samples.
    //
    // The resampler reads the input at `alpha * output_index`, with
    // `alpha = 1 / scale`, so output frame `i` carries the material of input
    // frame `i * alpha`. The flag for that frame has to come from that same
    // input frame; taking `i / alpha` instead would attach every flag to the
    // wrong span, and the error is invisible when the scale is one and grows
    // with the drift when it is not.
    let quiet: Vec<bool> = db
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let src = (i as f64 * alpha).floor() as usize;
            let was_quiet = env.quiet.get(src).copied().unwrap_or(false);
            was_quiet && *v < env.quiet_threshold_db
        })
        .collect();

    Envelope {
        rate_hz: env.rate_hz,
        db,
        norm,
        quiet,
        noise_floor_db: env.noise_floor_db,
        quiet_threshold_db: env.quiet_threshold_db,
        noise_floor_degenerate: env.noise_floor_degenerate,
        duration_secs: env.duration_secs * scale,
    }
}
