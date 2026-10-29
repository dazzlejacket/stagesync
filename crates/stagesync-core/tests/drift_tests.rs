//! Drift-estimation tests.
//!
//! These cover the part of the pipeline that decides whether a drift is *measured* or
//! only guessed: the waveform-domain estimator, the uncertainty of a fitted slope, and
//! the fallback to an offset-only correction when the evidence is not good enough.
//!
//! Everything is generated in Rust inside the test process. Nothing here depends on
//! Python, on the network, or on any checked-in media.

mod common;

use std::path::PathBuf;

use common::{
    analyse, config, make_source, reference_file, reference_signal, write_wav, SourceSpec,
};
use stagesync_core::audio::WavFormat;
use stagesync_core::matching::model::LinearModel;
use stagesync_core::matching::{DriftMethod, DriftStatus, MatchMethod};
use stagesync_core::render::{render_source_aligned, Layout, RenderOptions};
use stagesync_core::verify::{verify_corrected, VerifyConfig};
use tempfile::TempDir;

const RATE: u32 = 8000;
const SOURCE_RATE: u32 = 7200;

/// A recording short enough that the envelope cannot resolve a small drift on its own.
const SHORT_DURATION: f64 = 180.0;

/// The drift the short fixture carries, in parts per million.
const SHORT_DRIFT_PPM: f64 = 35.0;

/// Build a source against a reference of a given length.
fn build_source(dir: &TempDir, name: &str, reference_duration: f64, spec: SourceSpec) -> PathBuf {
    let reference = reference_signal(RATE, reference_duration, 0xC0FFEE);
    let (samples, _) = make_source(&reference, RATE, spec, 0xBEEF);
    let path = dir.path().join(name);
    write_wav(&path, &samples, spec.out_rate, WavFormat::Float32);
    path
}

/// The short fixture: 180 s with a 35 ppm drift.
fn short_fixture(dir: &TempDir) -> (PathBuf, common::GroundTruth) {
    let scale = 1.0 + SHORT_DRIFT_PPM / 1_000_000.0;
    let spec = SourceSpec {
        scale,
        offset: 37.5,
        out_rate: SOURCE_RATE,
        gain_db: -20.0,
        duration_secs: None,
    };
    let reference = reference_file(RATE, SHORT_DURATION, 0xC0FFEE);
    assert!(reference.exists(), "the cached reference should exist");
    let source = build_source(dir, "short_source.wav", SHORT_DURATION, spec);
    (
        source,
        common::GroundTruth {
            scale,
            offset: spec.offset,
            out_rate: SOURCE_RATE,
            ref_rate: RATE,
        },
    )
}
#[test]
fn the_drift_of_a_short_recording_is_measured_on_the_raw_waveform() {
    let dir = TempDir::new().expect("temp dir");
    let (source, _truth) = short_fixture(&dir);

    let report = analyse(
        &reference_file(RATE, SHORT_DURATION, 0xC0FFEE),
        &[source],
        &config(),
    );
    let alignment = &report.sources[0];

    let assessment = alignment
        .drift
        .as_ref()
        .expect("the analysis should reach a verdict on the drift");
    assert_eq!(
        assessment.status,
        DriftStatus::Established,
        "the drift of a {SHORT_DURATION:.0} s recording should be established: {:?}",
        assessment.notes
    );
    assert_eq!(
        assessment.method,
        DriftMethod::Waveform,
        "the envelope alone cannot resolve {SHORT_DRIFT_PPM} ppm over {SHORT_DURATION:.0} s, \
         so the waveform estimate should have decided it: {:?}",
        assessment.notes
    );

    let model = alignment
        .model()
        .expect("the analysis should have produced a model");
    let measured = model.drift_ppm();
    let error_ppm = measured - SHORT_DRIFT_PPM;
    assert!(
        error_ppm.abs() < 2.0,
        "the drift was measured as {measured:+.3} ppm, which is {error_ppm:+.3} ppm from the truth"
    );

    // What matters for the audio is the drift accumulated over the whole source.
    let accumulated_error_ms =
        model.accumulated_drift_ms(0.0, 120.0) - SHORT_DRIFT_PPM / 1_000_000.0 * 120.0 * 1000.0;
    assert!(
        accumulated_error_ms.abs() < 0.25,
        "the accumulated drift error over 120 s was {accumulated_error_ms:+.4} ms"
    );
}

#[test]
fn the_waveform_measurement_corrects_an_envelope_estimate_that_looks_precise() {
    let dir = TempDir::new().expect("temp dir");
    let (source, _truth) = short_fixture(&dir);
    let reference = reference_file(RATE, SHORT_DURATION, 0xC0FFEE);

    // With the waveform measurement switched off, the drift rests on the envelope
    // alone. The envelope's own uncertainty is small — the residuals are small — but
    // the residuals are not independent of one another, so that uncertainty is a floor
    // rather than a guarantee, and the figure it produces is off.
    let mut cfg = config();
    cfg.measure_waveform_drift = false;
    let without = analyse(&reference, std::slice::from_ref(&source), &cfg);
    let envelope_drift = without.sources[0].model().expect("a model").drift_ppm();
    let envelope_error = envelope_drift - SHORT_DRIFT_PPM;
    assert!(
        envelope_error.abs() > 5.0,
        "the envelope estimate was {envelope_drift:+.3} ppm, which is {envelope_error:+.3} ppm from the \
         truth; the fixture is meant to be a case where the envelope looks precise and is not"
    );

    // With the waveform measurement switched on, the drift is corrected.
    let with = analyse(&reference, &[source], &config());
    let assessment = with.sources[0].drift.as_ref().expect("a verdict");
    assert_eq!(assessment.method, DriftMethod::Waveform);
    let corrected = with.sources[0].model().expect("a model").drift_ppm();
    assert!(
        (corrected - SHORT_DRIFT_PPM).abs() < 2.0,
        "the corrected drift was {corrected:+.3} ppm, which is not close enough to the truth"
    );
}

#[test]
fn offset_only_synchronisation_can_be_asked_for() {
    let dir = TempDir::new().expect("temp dir");
    let (source, _truth) = short_fixture(&dir);

    let mut cfg = config();
    cfg.force_offset_only = true;

    let report = analyse(
        &reference_file(RATE, SHORT_DURATION, 0xC0FFEE),
        &[source],
        &cfg,
    );
    let alignment = &report.sources[0];

    let assessment = alignment
        .drift
        .as_ref()
        .expect("the analysis should reach a verdict on the drift");
    assert_eq!(assessment.status, DriftStatus::OffsetOnly);
    assert_eq!(assessment.method, DriftMethod::OffsetOnly);

    let model = alignment.model().expect("a model");
    assert_eq!(model.scale, 1.0, "the rate was changed anyway");
    assert!(
        assessment
            .notes
            .iter()
            .any(|note| note.contains("offset-only synchronisation was requested")),
        "the override should be recorded: {:?}",
        assessment.notes
    );
    // The evidence is still reported, so the choice is visible rather than hidden.
    assert!(
        assessment.envelope.is_some() || assessment.waveform.is_some(),
        "the drift evidence should still be reported"
    );
}

#[test]
fn the_waveform_probes_are_holdout_positions() {
    let dir = TempDir::new().expect("temp dir");
    let (source, _truth) = short_fixture(&dir);

    let report = analyse(
        &reference_file(RATE, SHORT_DURATION, 0xC0FFEE),
        &[source],
        &config(),
    );
    let alignment = &report.sources[0];

    let envelope_times: Vec<f64> = alignment
        .points
        .iter()
        .filter(|p| p.method == MatchMethod::FineEnvelope)
        .map(|p| p.source_time)
        .collect();
    let waveform_times: Vec<f64> = alignment
        .points
        .iter()
        .filter(|p| p.method == MatchMethod::WaveformPhat)
        .map(|p| p.source_time)
        .collect();

    assert!(
        !waveform_times.is_empty(),
        "the waveform estimator should have probed the recording"
    );
    for probe in &waveform_times {
        for fitted in &envelope_times {
            assert!(
                (probe - fitted).abs() > 0.5,
                "the waveform probe at {probe:.3} s sits on an envelope probe at {fitted:.3} s, \
                 so it is not a hold-out position"
            );
        }
    }
}

#[test]
fn the_correction_that_was_applied_leaves_the_drift_error_undetectable() {
    let dir = TempDir::new().expect("temp dir");
    let (source, _truth) = short_fixture(&dir);

    let report = analyse(
        &reference_file(RATE, SHORT_DURATION, 0xC0FFEE),
        &[source],
        &config(),
    );
    let alignment = &report.sources[0];
    let model = alignment.model().expect("a model");

    // The model predicts the reference time of each measured point. The residual
    // against it is what the hold-out check later measures on the rendered audio.
    let residuals: Vec<f64> = alignment
        .points
        .iter()
        .filter(|p| p.method == MatchMethod::WaveformPhat && p.confidence.score >= 0.5)
        .map(|p| (p.reference_time - model.reference_time(p.source_time)) * 1000.0)
        .collect();
    assert!(!residuals.is_empty(), "no waveform probe was trusted");

    let worst = residuals.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(
        worst < 1.0,
        "the applied model disagrees with the waveform measurements by up to {worst:.4} ms"
    );
}

#[test]
fn the_holdout_drift_error_has_no_systematic_bias() {
    // The hold-out measures how wrong the applied drift is by looking at the slope of
    // the residuals on the *rendered* file. If the render or the measurement had a
    // systematic rate error of its own, the figure would be biased, and it would be
    // used to contradict the model. Rendering with the exact ground truth therefore
    // has to come out as "no drift error".
    let dir = TempDir::new().expect("temp dir");
    let (source, truth) = short_fixture(&dir);
    let reference = reference_file(RATE, SHORT_DURATION, 0xC0FFEE);

    let model = LinearModel::new(truth.scale, truth.offset);
    let corrected = dir.path().join("corrected.wav");
    render_source_aligned(
        &source,
        &model,
        &corrected,
        &RenderOptions {
            layout: Layout::SourceAligned,
            sample_rate: Some(SOURCE_RATE),
            format: WavFormat::Float32,
            ..RenderOptions::default()
        },
    )
    .expect("the source can be rendered");

    let result = verify_corrected(
        &corrected,
        &reference,
        &model,
        SHORT_DURATION,
        &VerifyConfig::default(),
    )
    .expect("the corrected file can be verified");

    let error = result
        .drift_error_ppm
        .expect("the hold-out should give a drift error");
    assert!(
        error.abs() < 1.0,
        "rendering with the exact model gave a drift error of {error:+.3} ppm, which means the \
         measurement has a systematic bias of that size"
    );
}
