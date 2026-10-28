//! End-to-end alignment tests: offsets, drift, sample rates, levels and degradation.
//!
//! Every fixture is synthesised with an exact ground truth, so the assertions can be
//! about real numbers rather than "it looked right".

mod common;

use std::path::PathBuf;

use common::{
    analyse, config, make_source, point_errors_ms, reference_file, reference_signal,
    report_accuracy, write_wav, GroundTruth, SourceSpec,
};
use stagesync_core::audio::WavFormat;
use tempfile::TempDir;

/// The rate the general fixtures run at.
///
/// It is deliberately not 48 kHz: the matching features are envelopes at 100 Hz, so the
/// carrier rate does not change the difficulty, and a low rate keeps the suite fast.
/// One test uses the realistic 48 kHz / 44.1 kHz pair.
const RATE: u32 = 8000;
const SOURCE_RATE: u32 = 7200;
/// The length of the reference.
///
/// Long enough that the source contains a dozen fine-probe positions, so that "at
/// least five independent high-confidence positions" is a meaningful requirement.
const DURATION: f64 = 240.0;

/// Build a source file and return its path together with the ground truth.
fn build_source(
    dir: &TempDir,
    name: &str,
    scale: f64,
    offset: f64,
    out_rate: u32,
    gain_db: f64,
    duration_secs: Option<f64>,
) -> (PathBuf, GroundTruth) {
    build_source_from(
        dir,
        name,
        DURATION,
        SourceSpec {
            scale,
            offset,
            out_rate,
            gain_db,
            duration_secs,
        },
    )
}

/// Build a source file against a reference of a given length.
fn build_source_from(
    dir: &TempDir,
    name: &str,
    reference_duration: f64,
    spec: SourceSpec,
) -> (PathBuf, GroundTruth) {
    let reference = reference_signal(RATE, reference_duration, 0xC0FFEE);
    let (samples, truth) = make_source(&reference, RATE, spec, 0xBEEF);
    let path = dir.path().join(name);
    write_wav(&path, &samples, spec.out_rate, WavFormat::Float32);
    (path, truth)
}

/// The alignment of a single source against the reference.
fn align(dir: &TempDir, name: &str, scale: f64, offset: f64, gain_db: f64) -> (GroundTruth, f64) {
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, truth) = build_source(dir, name, scale, offset, SOURCE_RATE, gain_db, None);
    let report = analyse(&reference, &[source], &config());
    assert_eq!(
        report.sources.len(),
        1,
        "expected exactly one source in the report"
    );
    let alignment = &report.sources[0];
    let errors = point_errors_ms(alignment, &truth);
    report_accuracy(name, &errors);
    common::describe(&report);
    (truth, errors.len() as f64)
}

#[test]
fn a_clean_offset_is_recovered_within_five_milliseconds() {
    let dir = TempDir::new().expect("temp dir");
    let (truth, count) = align(&dir, "clean.wav", 1.0, 37.5, -20.0);
    assert!(
        count >= 5.0,
        "expected at least 5 independent high-confidence positions, got {count}"
    );
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = dir.path().join("clean.wav");
    let report = analyse(&reference, &[source], &config());
    let alignment = &report.sources[0];
    let errors = point_errors_ms(alignment, &truth);
    let (p50, p95, max) = common::stats(&errors);
    assert!(
        p50 <= 5.0,
        "p50 error {p50:.3} ms exceeds the 5 ms target (errors: {errors:?})"
    );
    assert!(
        p95 <= 10.0,
        "p95 error {p95:.3} ms exceeds the 10 ms target (errors: {errors:?})"
    );
    assert!(max <= 25.0, "max error {max:.3} ms is implausibly large");
}

#[test]
fn a_negative_offset_is_recovered() {
    let dir = TempDir::new().expect("temp dir");
    // A negative offset means the source started before the reference did. The
    // reference therefore does not contain the beginning of the source, and the
    // analysis has to find the part that it does contain.
    let (truth, count) = align(&dir, "negative.wav", 1.0, -12.0, -20.0);
    assert!(count >= 1.0, "no points were measured at all");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = dir.path().join("negative.wav");
    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    let (p50, _, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
}

#[test]
fn a_multi_hour_offset_is_recovered() {
    let dir = TempDir::new().expect("temp dir");
    // Two hours into a reference that is a little over two hours long. The carrier
    // rate is low on purpose: the matching features are envelopes at 100 Hz, so a
    // 1 kHz carrier is plenty and it keeps the fixture cheap.
    const LOW_RATE: u32 = 1000;
    const LOW_SOURCE_RATE: u32 = 900;
    let reference_secs = 2.0 * 3600.0 + 200.0;

    let reference = reference_file(LOW_RATE, reference_secs, 0xC0FFEE);
    let reference_signal = common::reference_signal(LOW_RATE, reference_secs, 0xC0FFEE);
    let (samples, truth) = make_source(
        &reference_signal,
        LOW_RATE,
        SourceSpec {
            scale: 1.0,
            offset: 2.0 * 3600.0,
            out_rate: LOW_SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: Some(150.0),
        },
        0xBEEF,
    );
    let source = dir.path().join("hours.wav");
    write_wav(&source, &samples, LOW_SOURCE_RATE, WavFormat::Float32);

    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("multi-hour offset", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
    assert!(
        (truth.reference_time(100.0) - (2.0 * 3600.0 + 100.0)).abs() < 1e-6,
        "the ground truth itself is wrong"
    );
}

/// The length of the fixture used for the drift measurements.
///
/// The drift is a *slope*, so its precision depends on how long the baseline is. The
/// point measurements carry roughly a millisecond of noise each, and a slope fitted
/// through them over a 215 s source is only good to about ±20 ppm. A 575 s source
/// brings that down to a few ppm, which is what a 15 ppm assertion needs to be a
/// meaningful test rather than a coin toss.
const DRIFT_DURATION: f64 = 600.0;

#[test]
fn drift_of_ten_thirty_five_and_minus_thirty_five_ppm_is_measured() {
    for ppm in [10.0f64, 35.0, -35.0] {
        let dir = TempDir::new().expect("temp dir");
        let scale = 1.0 + ppm / 1_000_000.0;
        let reference = reference_file(RATE, DRIFT_DURATION, 0xC0FFEE);
        let (source, truth) = build_source_from(
            &dir,
            &format!("drift_{ppm}.wav"),
            DRIFT_DURATION,
            SourceSpec {
                scale,
                offset: 25.0,
                out_rate: SOURCE_RATE,
                gain_db: -20.0,
                duration_secs: None,
            },
        );
        let report = analyse(&reference, &[source], &config());
        let alignment = &report.sources[0];
        let errors = point_errors_ms(alignment, &truth);
        report_accuracy(&format!("drift {ppm} ppm"), &errors);

        let measured = alignment
            .direct_model
            .map(|m| m.drift_ppm())
            .unwrap_or(f64::NAN);
        // The drift must be measured to the right sign and roughly the right size.
        assert!(
            (measured - truth.drift_ppm()).abs() <= 15.0,
            "measured drift {measured:.3} ppm does not match the truth {:.3} ppm",
            truth.drift_ppm()
        );
        let (p50, _, _) = common::stats(&errors);
        assert!(
            p50 <= 10.0,
            "p50 error {p50:.3} ms is too large for {ppm} ppm"
        );
    }
}

#[test]
fn a_thousand_ppm_drift_is_measured() {
    let dir = TempDir::new().expect("temp dir");
    let scale = 1.0 + 1000.0 / 1_000_000.0;
    let reference = reference_file(RATE, DRIFT_DURATION, 0xC0FFEE);
    let (source, truth) = build_source_from(
        &dir,
        "drift_1000.wav",
        DRIFT_DURATION,
        SourceSpec {
            scale,
            offset: 25.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
    );
    let report = analyse(&reference, &[source], &config());
    let alignment = &report.sources[0];
    let errors = point_errors_ms(alignment, &truth);
    report_accuracy("drift 1000 ppm", &errors);
    let measured = alignment
        .direct_model
        .map(|m| m.drift_ppm())
        .unwrap_or(f64::NAN);
    assert!(
        (measured - truth.drift_ppm()).abs() <= 60.0,
        "measured drift {measured:.3} ppm does not match the truth {:.3} ppm",
        truth.drift_ppm()
    );
}

#[test]
fn a_37_decibel_level_difference_does_not_break_the_match() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, truth) = build_source(&dir, "quiet.wav", 1.0, 37.5, SOURCE_RATE, -37.0, None);
    let report = analyse(&reference, &[source], &config());
    let alignment = &report.sources[0];
    let errors = point_errors_ms(alignment, &truth);
    report_accuracy("level -37 dB", &errors);
    assert!(
        errors.len() >= 5,
        "expected at least 5 points, got {}",
        errors.len()
    );
    let (p50, p95, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
    assert!(p95 <= 25.0, "p95 error {p95:.3} ms is too large");

    // The correction must not have amplified the source.
    let model = alignment
        .calibrated_model
        .or(alignment.direct_model)
        .expect("a model");
    assert!(
        model.scale.abs() > 0.99 && model.scale < 1.01,
        "the model implies a large scale change of {} for a level-only difference",
        model.scale
    );
}

#[test]
fn a_different_sample_rate_is_handled() {
    let dir = TempDir::new().expect("temp dir");
    // The source runs faster than the reference.
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, truth) = build_source(&dir, "fast.wav", 1.0, 20.0, RATE * 2, -20.0, None);
    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("source at twice the reference rate", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
}

#[test]
fn a_realistic_48_and_44_1_kilohertz_pair_is_handled() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(48_000, 120.0, 0xC0FFEE);
    let reference_signal = common::reference_signal(48_000, 120.0, 0xC0FFEE);
    let (samples, truth) = make_source(
        &reference_signal,
        48_000,
        SourceSpec {
            scale: 1.0 + 35.0 / 1_000_000.0,
            offset: 18.0,
            out_rate: 44_100,
            gain_db: -20.0,
            duration_secs: Some(120.0),
        },
        0xBEEF,
    );
    let source = dir.path().join("realistic.wav");
    write_wav(&source, &samples, 44_100, WavFormat::Float32);

    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("48 kHz vs 44.1 kHz", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
}

#[test]
fn degraded_material_is_still_aligned_within_ten_milliseconds() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (mut samples, truth) = make_source(
        &reference_signal,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 25.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
        0xBEEF,
    );

    // A one-pole tilt: a crude EQ that colours the whole signal.
    let mut prev = 0.0f32;
    for v in samples.iter_mut() {
        let filtered = 0.7 * *v + 0.3 * prev;
        prev = *v;
        *v = filtered;
    }
    // Extra broadband noise at a level that is clearly audible but not destructive.
    let mut rng = common::Rng::new(0x1234);
    for v in samples.iter_mut() {
        *v += 0.02 * rng.next_f32();
    }

    let source = dir.path().join("degraded.wav");
    write_wav(&source, &samples, SOURCE_RATE, WavFormat::Float32);

    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("EQ + noise", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, _) = common::stats(&errors);
    assert!(
        p50 <= 10.0,
        "p50 error {p50:.3} ms is too large for degraded material"
    );
}

#[test]
fn silence_at_the_head_and_tail_does_not_break_the_match() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (mut samples, truth) = make_source(
        &reference_signal,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 25.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
        0xBEEF,
    );
    let quiet = (5.0 * SOURCE_RATE as f64) as usize;
    for v in samples.iter_mut().take(quiet) {
        *v = 0.0;
    }
    let len = samples.len();
    for v in samples.iter_mut().skip(len - quiet) {
        *v = 0.0;
    }

    let source = dir.path().join("silence.wav");
    write_wav(&source, &samples, SOURCE_RATE, WavFormat::Float32);

    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("head and tail silence", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, _) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
}

#[test]
fn repeated_but_varied_choruses_are_not_matched_to_the_wrong_place() {
    let dir = TempDir::new().expect("temp dir");
    // The fixture's chord progression comes round again with different dynamics, so
    // several positions sound similar. The matcher has to pick the right one.
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (samples, truth) = make_source(
        &reference_signal,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 30.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
        0xBEEF,
    );
    let source = dir.path().join("chorus.wav");
    write_wav(&source, &samples, SOURCE_RATE, WavFormat::Float32);

    let report = analyse(&reference, &[source], &config());
    let errors = point_errors_ms(&report.sources[0], &truth);
    report_accuracy("repeated choruses", &errors);
    assert!(!errors.is_empty(), "no points were measured");
    let (p50, _, max) = common::stats(&errors);
    assert!(p50 <= 10.0, "p50 error {p50:.3} ms is too large");
    assert!(
        max <= 50.0,
        "max error {max:.3} ms suggests the matcher locked onto the wrong chorus"
    );
}

#[test]
fn the_log_mel_cross_check_agrees_with_the_envelope_fit_on_identical_material() {
    let dir = TempDir::new().expect("temp dir");
    // The log-mel bands reach up to 8 kHz, so this case runs at a realistic carrier
    // rate: at 8 kHz half of the bands would be empty and the cross-check would have
    // nothing to correlate.
    let reference = reference_file(48_000, 120.0, 0xC0FFEE);
    let reference_signal = common::reference_signal(48_000, 120.0, 0xC0FFEE);
    // The source is the reference, re-sampled and barely noisy: the two feature
    // families must agree closely.
    let (samples, truth) = make_source(
        &reference_signal,
        48_000,
        SourceSpec {
            scale: 1.0,
            offset: 30.0,
            out_rate: 44_100,
            gain_db: -6.0,
            duration_secs: None,
        },
        0xBEEF,
    );
    let source = dir.path().join("mel.wav");
    write_wav(&source, &samples, 44_100, WavFormat::Float32);

    let mut cfg = config();
    cfg.compute_log_mel = true;
    let report = analyse(&reference, &[source], &cfg);
    let alignment = &report.sources[0];

    let envelope_errors = point_errors_ms(alignment, &truth);
    report_accuracy("envelope fit (log-mel enabled)", &envelope_errors);
    assert!(
        !envelope_errors.is_empty(),
        "no envelope points were measured"
    );
    let (p50, _, _) = common::stats(&envelope_errors);
    assert!(p50 <= 10.0, "p50 envelope error {p50:.3} ms is too large");

    // The spectral cross-check is a second opinion. It is not the measurement the
    // model was fitted with, so it is held to a looser bound — but it must not
    // contradict the envelope fit either.
    let spectral_errors = common::spectral_errors_ms(alignment, &truth);
    report_accuracy("log-mel cross-check", &spectral_errors);
    assert!(
        !spectral_errors.is_empty(),
        "the log-mel cross-check produced no usable points"
    );
    let (p50, p95, max) = common::stats(&spectral_errors);
    assert!(
        p50 <= 50.0,
        "the spectral cross-check disagrees with the truth at a p50 of {p50:.3} ms"
    );
    assert!(
        max <= 250.0,
        "the spectral cross-check disagrees with the truth by up to {max:.3} ms"
    );
    let _ = p95;
}
