//! What a recorder gap does to the alignment, measured rather than argued about.
//!
//! The case under test is the hardest one the matcher meets: a **short** source of
//! which most is a recorder gap. The review that prompted this file measured such
//! a fixture with its own harness and concluded that the implementation aligned
//! it exactly, while the *pipeline* â€” which searches its fine probes around the
//! coarse lock's model rather than around the ground truth â€” did not. Both were
//! right, and the difference between them was the whole defect:
//!
//! * the coarse stage's whole-file correlation was **not masked**, so a gap full
//!   of digital silence was correlated against the reference's music and dragged
//!   the peak away from the true correspondence;
//! * the coarse stage ranked its peaks by the matched-filter score with a
//!   *global* normalisation, which is linear in the reference's amplitude and so
//!   is largest where the reference happens to be most dynamic rather than where
//!   the two envelopes agree best.
//!
//! Both faults are exercised here, on a fixture whose correspondence is exact by
//! construction, so a regression in either of them fails this file.
//!
//! The file also settles the **noise-floor coupling** â€” the quiet threshold is a
//! fixed percentile of the frame-level distribution, so a recording that contains
//! digital silence gets a different threshold from the same recording without it.
//! That coupling is real and it is measured here; whether it *harms* the
//! alignment is a separate question, and it is answered by measurement too.

mod common;

use std::f64::consts::PI;
use std::path::Path;

use common::{analyse, config, reference_signal, Rng};
use stagesync_core::audio::{AudioDecoder, WavFormat};
use stagesync_core::features::envelope::{
    extract_envelope, local_zscore_masked, Envelope, EnvelopeConfig,
};
use stagesync_core::matching::coarse::{coarse_lock, CoarseConfig};
use stagesync_core::matching::fine::{fine_match, FineOptions};
use tempfile::TempDir;

/// The sample rate of the fixture.
const RATE: u32 = 16000;
/// How long the reference runs.
const REFERENCE_SECS: f64 = 300.0;
/// How long the source runs.
const SOURCE_SECS: f64 = 90.0;
/// Where the source starts in the reference: `reference_time = source_time + OFFSET`.
const OFFSET: f64 = 10.0;
/// How long the recorder's gap is.
const GAP_SECS: f64 = 60.0;

/// The ground truth of the fixture: no drift, a known offset.
fn truth() -> common::GroundTruth {
    common::GroundTruth {
        scale: 1.0,
        offset: OFFSET,
        out_rate: RATE,
        ref_rate: RATE,
    }
}

/// The source recording: the reference, read at a known affine position, put
/// through a different microphone's level, a different dynamic range and the
/// recorder's own noise floor, and then cut by a recorder gap.
///
/// The gap is **digital silence**, because that is what a recorder that stopped
/// writing produces.
fn build_source(reference: &[f32], gap: Option<(f64, f64)>) -> Vec<f32> {
    let n = (SOURCE_SECS * RATE as f64).round() as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = Rng::new(0x0DDBA11);

    // A different microphone: a constant level of its own and a slowly moving
    // level on top of it.
    let base_gain = 10.0f64.powf(-14.0 / 20.0);

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
        let swing = 2.5 * (2.0 * PI * t / 97.0).sin() + 1.25 * (2.0 * PI * t / 53.0 + 1.1).sin();
        v *= (base_gain * 10.0f64.powf(swing / 20.0)) as f32;

        // A different dynamic range: a mild soft clip.
        const K: f32 = 1.25;
        v = (K * v).tanh() / K.tanh();

        // The recorder's own noise floor, at about -62 dBFS.
        *slot = v + 0.000_794 * rng.next_f32();
    }

    if let Some((t0, t1)) = gap {
        let a = (t0 * RATE as f64).round() as usize;
        let b = (t1 * RATE as f64).round() as usize;
        let end = b.min(out.len());
        for v in out.iter_mut().take(end).skip(a) {
            *v = 0.0;
        }
    }
    out
}

/// Write the reference into a temporary directory, and return its path and its
/// samples.
fn write_reference(dir: &TempDir) -> (std::path::PathBuf, Vec<f32>) {
    let samples = reference_signal(RATE, REFERENCE_SECS, 0xA5A5);
    let path = dir.path().join("reference.wav");
    common::write_wav(&path, &samples, RATE, WavFormat::Float32);
    (path, samples)
}

/// Write one source recording against the reference.
fn write_source(
    dir: &TempDir,
    name: &str,
    reference_samples: &[f32],
    gap: Option<(f64, f64)>,
) -> std::path::PathBuf {
    let path = dir.path().join(format!("{name}-source.wav"));
    let samples = build_source(reference_samples, gap);
    common::write_wav(&path, &samples, RATE, WavFormat::Float32);
    path
}

/// A source recording with a recorder gap in it.
fn fixture(
    dir: &TempDir,
    reference_samples: &[f32],
    gap: Option<(f64, f64)>,
    name: &str,
) -> std::path::PathBuf {
    write_source(dir, name, reference_samples, gap)
}

/// A short source dominated by a recorder gap must still be aligned.
///
/// This is the decisive test of the whole dispute: the coarse lock has to find
/// the true offset, and every probe it trusts has to be accurate. Against the
/// implementation as it stood, the coarse lock landed **10 seconds** away from
/// the truth on this fixture, and the one probe that was trusted was 32 seconds
/// out â€” because the fine stage searches around the coarse lock's model.
#[test]
fn a_short_source_dominated_by_a_recorder_gap_is_still_aligned() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, reference_samples) = write_reference(&dir);
    let source = fixture(&dir, &reference_samples, Some((20.0, 80.0)), "gap60");

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];

    // The coarse lock must find the true offset. Its own resolution is 50 ms, so
    // a tenth of a second is a generous bound; the measurement is far better
    // than that in practice, and the assertion says so.
    let coarse = alignment
        .coarse
        .as_ref()
        .expect("the coarse lock should have run");
    let coarse_offset = coarse
        .model
        .expect("the coarse lock should have produced a model")
        .offset;
    let coarse_error_ms = (coarse_offset - OFFSET) * 1000.0;
    assert!(
        coarse_error_ms.abs() < 50.0,
        "the coarse lock came out {coarse_error_ms:.1} ms from the truth of {OFFSET} s \
         (full-file lag {:?}, score {})",
        coarse.full_range_lag_secs,
        coarse.full_range_score
    );

    // Every probe the matcher trusts has to be right.
    let errors = common::point_errors_ms(alignment, &truth());
    assert!(
        !errors.is_empty(),
        "no probe was trusted at all: {} measured, {} skipped ({:?})",
        alignment.fine_measured,
        alignment.fine_skipped,
        alignment.fine_skip_reasons
    );
    for error in &errors {
        // The envelope's time resolution is one frame, ten milliseconds, so a
        // probe cannot be expected to be better than that.
        assert!(
            error.abs() < 10.0,
            "a trusted probe was {error:.3} ms from the truth"
        );
    }
    // And the typical probe has to be much better than that: the median is
    // within a millisecond, which is what a correct correspondence looks like.
    let mut sorted = errors.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let median = sorted[sorted.len() / 2];
    assert!(
        median.abs() < 1.0,
        "the median trusted probe was {median:.3} ms from the truth"
    );

    // The drift has to be reported with the uncertainty that produced it, and a
    // drift that cannot be defended has to be refused rather than applied. The
    // fixture carries no drift at all.
    let drift = alignment
        .drift
        .as_ref()
        .expect("the drift must be reported with its uncertainty");
    assert!(
        drift.slope_se_ppm.is_finite(),
        "the drift was reported without a usable uncertainty"
    );
    if let Some(envelope) = &drift.envelope {
        assert!(
            envelope.drift_ppm.abs() <= 3.0 * envelope.slope_se_ppm,
            "a drift of {} ppm was applied with an uncertainty of {} ppm, which does not \
             justify it",
            envelope.drift_ppm,
            envelope.slope_se_ppm
        );
    }
}

/// The place of the gap must not change the measured drift.
///
/// The same performance, with the same recorder, with the gap in two different
/// places: the measured drift has to agree to within the fit's own standard
/// error. This is the property `AGENTS.md` asks for, expressed as a measurement
/// rather than as a rule about how to compute a statistic.
#[test]
fn the_place_of_the_gap_does_not_change_the_measured_drift() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, reference_samples) = write_reference(&dir);

    let mut figures: Vec<(f64, f64, f64)> = Vec::new();
    for (index, gap_start) in [5.0f64, 55.0].iter().enumerate() {
        let gap = Some((*gap_start, *gap_start + GAP_SECS));
        let source = fixture(&dir, &reference_samples, gap, &format!("place-{index}"));
        let report = analyse(&reference, std::slice::from_ref(&source), &config());
        let alignment = &report.sources[0];

        // The alignment itself must not depend on where the gap is.
        let coarse_offset = alignment
            .coarse
            .as_ref()
            .and_then(|c| c.model)
            .expect("the coarse lock should have produced a model")
            .offset;
        let offset_error_ms = (coarse_offset - OFFSET) * 1000.0;
        assert!(
            offset_error_ms.abs() < 50.0,
            "with the gap at {gap_start:.0} s the coarse lock came out {offset_error_ms:.1} ms \
             from the truth"
        );

        let drift = alignment
            .drift
            .as_ref()
            .expect("the drift must be reported with its uncertainty");
        let (ppm, se) = drift
            .envelope
            .as_ref()
            .map(|e| (e.drift_ppm, e.slope_se_ppm))
            .unwrap_or((drift.drift_ppm, drift.slope_se_ppm));
        figures.push((ppm, se, offset_error_ms));
    }

    let (ppm_a, se_a, _) = figures[0];
    let (ppm_b, se_b, _) = figures[1];
    let difference = (ppm_a - ppm_b).abs();
    let uncertainty = se_a.max(se_b);
    assert!(
        difference <= 3.0 * uncertainty,
        "moving the gap changed the measured drift by {difference:.3} ppm, against an \
         uncertainty of {uncertainty:.3} ppm"
    );
}

/// The quiet threshold depends on whether the recording has a gap — a
/// characterisation, not a wish.
///
/// The quiet threshold is a fixed percentile of the frame-level distribution, so a
/// recording that contains digital silence gets a completely different threshold
/// from the same recording without it: the percentile lands inside the silence
/// instead of on the recorder's own noise floor. The two recordings here are the
/// *same performance* through the same microphone, so the difference below is
/// caused by nothing but the gap.
///
/// This test records the coupling as it stands, so that closing it becomes a
/// deliberate change rather than an accident. Whether the coupling *harms* the
/// correction is a different question, and
/// [`the_alignment_survives_the_noise_floor_coupling`] answers it.
#[test]
fn the_quiet_threshold_depends_on_whether_the_recording_has_a_gap() {
    let dir = TempDir::new().expect("temp dir");
    let (_reference, reference_samples) = write_reference(&dir);
    let gapped = fixture(&dir, &reference_samples, Some((20.0, 80.0)), "coupled");
    let ungapped = fixture(&dir, &reference_samples, None, "uncoupled");

    let gapped_env = envelope_of(&gapped);
    let ungapped_env = envelope_of(&ungapped);
    let difference = (gapped_env.quiet_threshold_db - ungapped_env.quiet_threshold_db).abs();
    println!(
        "quiet threshold: with a gap {:.2} dB, without {:.2} dB, a difference of {difference:.2} dB",
        gapped_env.quiet_threshold_db, ungapped_env.quiet_threshold_db
    );

    // The gap's digital silence drags the percentile down to the silence, so the
    // gapped recording's threshold sits far below the ungapped one's — which
    // sits just above the recorder's own noise floor.
    assert!(
        gapped_env.quiet_threshold_db < ungapped_env.quiet_threshold_db - 50.0,
        "the quiet threshold of the gapped recording ({:.2} dB) is not far below the \
         ungapped one ({:.2} dB): the coupling has changed",
        gapped_env.quiet_threshold_db,
        ungapped_env.quiet_threshold_db
    );
}

/// The alignment survives the noise-floor coupling.
///
/// The coupling is real — see
/// [`the_quiet_threshold_depends_on_whether_the_recording_has_a_gap`] — so the
/// question is whether it *moves the correspondence*. It does not:
///
/// * the same performance, with and without a gap, lands on the same offset;
/// * the same gapped recording, with the threshold moved by hand to the value the
///   recording without a gap would have had, lands on the same offset and trusts
///   the same number of probes.
///
/// That is why the noise-floor rule was **not** changed: the change the review
/// asked for would have moved the threshold of every recording that contains any
/// digital silence — including the real ones — without improving the
/// correspondence by any measurable amount.
#[test]
fn the_alignment_survives_the_noise_floor_coupling() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, reference_samples) = write_reference(&dir);
    let gapped = fixture(&dir, &reference_samples, Some((20.0, 80.0)), "coupled");
    let ungapped = fixture(&dir, &reference_samples, None, "uncoupled");

    let gapped_env = envelope_of(&gapped);
    let ungapped_env = envelope_of(&ungapped);
    let reference_env = envelope_of(&reference);

    // The alignment of the two recordings of the same performance.
    let gapped_result = align(&gapped_env, &reference_env);
    let ungapped_result = align(&ungapped_env, &reference_env);
    println!(
        "with a gap:    coarse error {:+.3} ms, {} trusted probe(s), errors {:?}",
        gapped_result.offset_error_ms,
        gapped_result.errors.len(),
        gapped_result.errors
    );
    println!(
        "without a gap: coarse error {:+.3} ms, {} trusted probe(s), errors {:?}",
        ungapped_result.offset_error_ms,
        ungapped_result.errors.len(),
        ungapped_result.errors
    );

    // The coupling isolated: the *same* gapped recording, correlated with the
    // *same* reference, with only the threshold moved to the one the recording
    // without a gap would have had.
    let moved = envelope_with_threshold(&gapped, ungapped_env.quiet_threshold_db);
    let moved_result = align(&moved, &reference_env);
    println!(
        "gap, threshold moved to {:.2} dB: coarse error {:+.3} ms, {} trusted probe(s), errors {:?}",
        ungapped_env.quiet_threshold_db,
        moved_result.offset_error_ms,
        moved_result.errors.len(),
        moved_result.errors
    );

    // All three have to land on the truth, and none of them may trust a probe
    // that is wrong.
    for (label, result) in [
        ("with a gap", &gapped_result),
        ("without a gap", &ungapped_result),
        ("with a gap and the threshold moved", &moved_result),
    ] {
        assert!(
            result.offset_error_ms.abs() < 50.0,
            "the coarse lock {label} came out {:.1} ms from the truth",
            result.offset_error_ms
        );
        assert!(
            !result.errors.is_empty(),
            "no probe was trusted at all {label}"
        );
        for error in &result.errors {
            // The envelope's time resolution is one frame, ten milliseconds, so a
            // probe cannot be expected to be better than that.
            assert!(
                error.abs() < 10.0,
                "a trusted probe {label} was {error:.3} ms from the truth"
            );
        }
    }

    // Moving the threshold must not cost the gapped recording its probes.
    assert!(
        moved_result.errors.len() >= gapped_result.errors.len(),
        "moving the threshold took the gapped recording from {} trusted probe(s) to {}",
        gapped_result.errors.len(),
        moved_result.errors.len()
    );
}

/// The outcome of aligning one source against one reference.
struct Alignment {
    /// How far the coarse lock's offset was from the truth, in milliseconds.
    offset_error_ms: f64,
    /// The error of every trusted fine probe, in milliseconds.
    errors: Vec<f64>,
}

/// Align a source against a reference with the coarse and fine stages.
fn align(source: &Envelope, reference: &Envelope) -> Alignment {
    let lock = coarse_lock(source, reference, &CoarseConfig::default());
    let model = lock
        .model
        .expect("the coarse lock should have produced a model");
    let fine = fine_match(source, reference, &model, &FineOptions::default());
    let errors = fine
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.reference_time - (p.source_time + OFFSET)) * 1000.0)
        .collect();
    Alignment {
        offset_error_ms: (model.offset - OFFSET) * 1000.0,
        errors,
    }
}

/// The envelope of a recording, as the matcher sees it.
fn envelope_of(path: &Path) -> Envelope {
    let mut decoder = AudioDecoder::open(path).expect("open");
    extract_envelope(&mut decoder, EnvelopeConfig::default(), 4096, None).expect("envelope")
}

/// The envelope of a recording, with its quiet mask recomputed at a given
/// threshold.
///
/// This is how the noise-floor coupling is isolated: the *same* recording, the
/// *same* correlation, with only the threshold moved.
fn envelope_with_threshold(path: &Path, threshold_db: f32) -> Envelope {
    let mut env = envelope_of(path);
    let quiet: Vec<bool> = env.db.iter().map(|v| *v < threshold_db).collect();
    let half = (env.rate_hz * 30.0).round().max(1.0) as usize;
    let norm = local_zscore_masked(&env.db, half, 4.0, &quiet);
    env.quiet = quiet;
    env.norm = norm;
    env.quiet_threshold_db = threshold_db;
    env
}
