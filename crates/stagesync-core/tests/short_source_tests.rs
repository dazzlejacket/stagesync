//! The small-sample regime: a recording with only a handful of measurements.
//!
//! Everything in this file is about the case where a source is **short**. A short
//! recording yields a handful of probes rather than hundreds, and in that regime
//! the estimators behave differently: the robust fit has few points to work with,
//! and one wrong measurement can move the answer a long way.
//!
//! The defect this file exists for was measured, not imagined. With three or four
//! trusted probes, Theil–Sen's estimator takes the median of the pairwise slopes;
//! when one of the points is grossly wrong, three of the six slopes belong to the
//! outlier and the median of the sorted list can land on one of them. The fit then
//! looks perfectly ordinary — a line, a residual, a standard error — while it
//! describes a recording that sits seconds away from where it belongs. The drift
//! was caught by the signal-to-noise rule; the **offset** was not, and the offset
//! is the thing a listener notices.
//!
//! So the protection has to be about *corroboration*: a line that most of the
//! measurements do not support is refused, and the placement falls back to the
//! coarse lock, which came from an independent estimator. And the offset-only
//! fallback has to be robust on its own, because it is what is left when the drift
//! cannot be defended.

mod common;

use common::{analyse, config, point_errors_ms, reference_signal, stats};
use stagesync_core::audio::WavFormat;
use stagesync_core::matching::drift::fit_offset_only;
use stagesync_core::matching::fine::corroboration_support;
use stagesync_core::matching::regression::robust_fit;
use stagesync_core::matching::LinearModel;
use tempfile::TempDir;

/// The sample rate of the fixture.
const RATE: u32 = 16000;
/// How long the reference runs.
const REFERENCE_SECS: f64 = 240.0;
/// How long the short source runs.
const SOURCE_SECS: f64 = 95.0;
/// Where the source starts in the reference: `reference_time = source_time + OFFSET`.
const OFFSET: f64 = 37.5;

/// The ground truth of the fixture: no drift, a known offset.
fn truth() -> common::GroundTruth {
    common::GroundTruth {
        scale: 1.0,
        offset: OFFSET,
        out_rate: RATE,
        ref_rate: RATE,
    }
}

/// A source recording: the reference, read at a known affine position, through a
/// different microphone's level and dynamic range, with the recorder's own noise
/// floor on top.
fn build_source(reference: &[f32]) -> Vec<f32> {
    let n = (SOURCE_SECS * RATE as f64).round() as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = common::Rng::new(0x51A7C0DE);
    let base_gain = 10.0f64.powf(-11.0 / 20.0);

    for (j, slot) in out.iter_mut().enumerate() {
        let t = j as f64 / RATE as f64;
        let pos = (t + OFFSET) * RATE as f64;
        let mut v = if pos >= 0.0 && pos < reference.len() as f64 {
            let i = pos.floor() as usize;
            let frac = (pos - i as f64) as f32;
            let a = reference[i];
            let b = reference.get(i + 1).copied().unwrap_or(a);
            a * (1.0 - frac) + b * frac
        } else {
            0.0
        };
        v *= base_gain as f32;
        const K: f32 = 1.2;
        v = (K * v).tanh() / K.tanh();
        *slot = v + 0.000_6 * rng.next_f32();
    }
    out
}

/// Write the reference and the source into a temporary directory.
fn fixture(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let samples = reference_signal(RATE, REFERENCE_SECS, 0xBEEF01);
    let reference = dir.path().join("reference.wav");
    common::write_wav(&reference, &samples, RATE, WavFormat::Float32);

    let source = dir.path().join("short-source.wav");
    let source_samples = build_source(&samples);
    common::write_wav(&source, &source_samples, RATE, WavFormat::Float32);

    (reference, source)
}

/// A line that only two of four measurements support must be refused.
///
/// This is the defect, stated as a property of the estimator rather than as a
/// story about a recording. The four measurements below are three good ones and
/// one that is 11.5 seconds out — the shape the second audit measured. The robust
/// fit is free to return whatever it likes; what matters is that the result is
/// not *supported*, and a fit that is not supported may not be applied.
#[test]
fn a_line_that_only_two_of_four_measurements_support_is_refused() {
    // Three good measurements on the line `reference_time = source_time + 10`,
    // and one that is 11.5 s late.
    let measurements = vec![
        (0.0_f64, 10.0_f64),
        (15.0, 25.0),
        (30.0, 40.0),
        (45.0, 66.5),
    ];

    let fit = robust_fit(&measurements, 3.0, 6).expect("the estimator returns a line");
    let (support, count) = corroboration_support(&measurements, &fit.model(), 0.100);
    assert_eq!(support.len(), measurements.len());

    // The line the estimator found must not be taken on trust: at most a third of
    // the measurements may disagree with it, and here half of them do.
    let allowed = (measurements.len() - 1) / 3;
    assert!(
        count + allowed < measurements.len(),
        "the refusal did not happen: {count} of {} measurements support the line, \
         and up to {allowed} may disagree",
        measurements.len()
    );
}

/// The same test the other way round: a line that the measurements *do* support
/// has to be accepted, or the protection would throw away good work.
#[test]
fn a_line_that_the_measurements_support_is_accepted() {
    let measurements = vec![
        (0.0_f64, 10.004_f64),
        (15.0, 25.003),
        (30.0, 40.006),
        (45.0, 55.002),
        (60.0, 70.005),
    ];

    let fit = robust_fit(&measurements, 3.0, 6).expect("the estimator returns a line");
    let (_support, count) = corroboration_support(&measurements, &fit.model(), 0.100);
    let allowed = (measurements.len() - 1) / 3;
    assert!(
        count + allowed >= measurements.len(),
        "a well-supported line was refused: only {count} of {} measurements support it",
        measurements.len()
    );
}

/// An offset measured from four measurements, one of them grossly wrong, has to
/// be the **median** of the lags.
///
/// A mean is not robust: one measurement 11.5 s out drags a four-point mean by
/// nearly three seconds, and that is the whole offset. The median ignores it.
#[test]
fn an_offset_measured_with_one_grossly_wrong_measurement_is_the_median() {
    let measurements = vec![
        (0.0_f64, 10.0_f64),
        (15.0, 25.0),
        (30.0, 40.0),
        (45.0, 66.5),
    ];

    let model = fit_offset_only(&measurements).expect("an offset");
    assert!(
        (model.offset - 10.0).abs() < 1e-9,
        "the offset was {} s, and the three good measurements say 10 s",
        model.offset
    );

    // The mean, for comparison: this is the number the offset used to be.
    let mean = measurements.iter().map(|(x, y)| y - x).sum::<f64>() / 4.0;
    assert!(
        (mean - 10.0).abs() > 1.0,
        "the mean of these lags is {mean:.3} s, which is what the offset used to be — \
         if it is close to 10 s the test is not exercising anything"
    );
}

/// A short recording must be placed where it belongs.
///
/// The end-to-end statement of the whole file: a source with only a handful of
/// probes has to come out at the right offset, and when it does not the report
/// has to say so rather than present a confident wrong answer.
#[test]
fn a_short_recording_is_placed_where_it_belongs() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, source) = fixture(&dir);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];
    common::describe(&report);

    let calibrated = alignment
        .calibrated_model
        .as_ref()
        .expect("a short recording must still produce a model");
    let offset_error_ms = (calibrated.offset - OFFSET) * 1000.0;
    assert!(
        offset_error_ms.abs() < 50.0,
        "the short recording was placed {offset_error_ms:.1} ms from the truth of {OFFSET} s"
    );

    // Every measurement the matcher trusted has to be right as well.
    let errors = point_errors_ms(alignment, &truth());
    assert!(
        !errors.is_empty(),
        "no probe was trusted at all: {} measured, {} skipped ({:?})",
        alignment.fine_measured,
        alignment.fine_skipped,
        alignment.fine_skip_reasons
    );
    let (p50, p95, max) = stats(&errors);
    assert!(
        max < 25.0,
        "a trusted probe was {max:.3} ms from the truth (p50 {p50:.3} ms, p95 {p95:.3} ms)"
    );
}

/// The end of a short recording has to be probed.
///
/// The fine stage's window has to lie inside the source, which means the final
/// half-window used to be unreachable: on a ninety-second recording that is a
/// sixth of the material, and it is the material a listener hears last.
#[test]
fn the_end_of_a_short_recording_is_probed() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, source) = fixture(&dir);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];

    let last_probe = alignment
        .points
        .iter()
        .filter(|p| p.method == stagesync_core::matching::MatchMethod::FineEnvelope)
        .map(|p| p.source_time)
        .fold(0.0_f64, f64::max);

    // The last probe's *window* has to reach the end of the recording. The window
    // is thirty seconds, so a probe in the last thirty seconds is enough.
    assert!(
        last_probe >= SOURCE_SECS - 30.0,
        "the last probe was at {last_probe:.1} s, and the recording runs to {SOURCE_SECS:.1} s"
    );
}

/// The model that is applied has to be one the measurements support.
///
/// This ties the protection to the thing a listener hears: the offset that is
/// applied to the recording must not be one the fine measurements contradict.
#[test]
fn the_applied_model_is_one_the_measurements_support() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, source) = fixture(&dir);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];

    let applied = alignment.calibrated_model.as_ref().expect("a model");
    let model = LinearModel::new(applied.scale, applied.offset);

    let trusted: Vec<(f64, f64)> = alignment
        .points
        .iter()
        .filter(|p| {
            p.method == stagesync_core::matching::MatchMethod::FineEnvelope
                && p.confidence.is_trustworthy()
        })
        .map(|p| (p.source_time, p.reference_time))
        .collect();

    if trusted.len() >= 3 {
        let (_support, count) = corroboration_support(&trusted, &model, 0.100);
        let allowed = (trusted.len() - 1) / 3;
        assert!(
            count + allowed >= trusted.len(),
            "the applied model is contradicted by its own measurements: only {count} of {} \
             support it",
            trusted.len()
        );
    }
}
