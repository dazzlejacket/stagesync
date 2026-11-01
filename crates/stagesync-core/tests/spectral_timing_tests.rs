//! Regression tests for the log-mel cross-check's frame-rate handling.
//!
//! The log-mel hop is rounded to whole samples, so the two files' frames are not
//! the same distance apart: a 44.1 kHz file's frames are 32.0017 ms apart and a
//! 48 kHz file's are 32 ms apart. The cross-check used to convert the
//! **reference's** frame indices with the **source's** rate, and to report the
//! source's frame indices with the nominal rate rather than the actual one. The
//! two mistakes together produced a reference time that was short by 143 ppm —
//! about 690 ms by the end of an eighty-minute recording — and made the
//! spectral cross-check disagree with the envelope fit on material where the two
//! estimators were in fact agreeing.
//!
//! The tests here check the cross-check against a correspondence that is known
//! exactly, rather than against the envelope: a signal defined in continuous
//! time and sampled at two different rates is the same performance, so the
//! answer is not a matter of opinion.

mod common;

use std::f64::consts::PI;
use std::path::Path;

use stagesync_core::audio::{AudioDecoder, WavFormat, WavWriter};
use stagesync_core::features::envelope::{extract_envelope, EnvelopeConfig};
use stagesync_core::features::mel::{extract_log_mel, LogMel, LogMelConfig};
use stagesync_core::matching::fine::{fine_match, spectral_offsets, FineOptions};
use stagesync_core::matching::model::LinearModel;
use tempfile::TempDir;

/// The rate at which the log-mel frames of a file actually sit.
fn expected_mel_rate(sample_rate: u32) -> f64 {
    let hop = ((32.0 / 1000.0) * sample_rate as f64).round() as usize;
    sample_rate as f64 / hop as f64
}

/// A note in the synthetic performance.
struct Note {
    /// When the note starts, in seconds of the performance.
    start: f64,
    /// How long it rings for.
    duration: f64,
    /// Exponential decay rate.
    decay: f64,
    /// The partials: frequency, amplitude and phase.
    partials: Vec<(f64, f64, f64)>,
}

/// A deterministic performance.
///
/// The notes are placed at irregular times and each carries its own inharmonic
/// chord, so the performance has a shape that does not repeat. A signal that
/// repeats gives the correlation several equally good peaks and the matcher
/// picks one of them at random; the test would then be measuring the tie-break
/// rather than the timing.
fn build_notes(count: usize) -> Vec<Note> {
    let mut seed = 20_261_031u64;
    let next = |seed: &mut u64| -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as f64) / ((1u64 << 31) as f64)
    };
    let mut notes = Vec::with_capacity(count);
    let mut time = 0.0;
    for _ in 0..count {
        time += 0.18 + 0.62 * next(&mut seed);
        let duration = 0.7 + 1.3 * next(&mut seed);
        let decay = 1.2 + 3.4 * next(&mut seed);
        let mut partials = Vec::with_capacity(5);
        for k in 0..5 {
            let freq = 140.0 + next(&mut seed) * 2600.0 + 61.0 * k as f64;
            let amp = 0.55 / (1.0 + k as f64) + 0.2 * next(&mut seed);
            let phase = next(&mut seed) * 2.0 * PI;
            partials.push((freq, amp, phase));
        }
        notes.push(Note {
            start: time,
            duration,
            decay,
            partials,
        });
    }
    notes
}

/// Render the performance into a buffer sampled at `rate`.
///
/// The performance is defined in continuous time, so the same notes sampled at
/// two different rates are the same performance — which is what makes the
/// correspondence between the two files a known quantity rather than a measured
/// one.
fn render(notes: &[Note], rate: u32, offset: f64, duration: f64) -> Vec<f32> {
    let frames = (duration * rate as f64) as usize;
    let mut out = vec![0.0f32; frames];
    for note in notes {
        let local_start = note.start - offset;
        let i0 = (local_start * rate as f64).floor().max(0.0) as usize;
        let i1 = (((local_start + note.duration) * rate as f64)
            .ceil()
            .max(0.0) as usize)
            .min(frames);
        for (freq, amp, phase) in &note.partials {
            let step = 2.0 * PI * freq / rate as f64;
            // The partial is a rotation, so it is stepped rather than evaluated:
            // the sine of half a million angles is not worth a library call per
            // sample.
            let (mut sin, mut cos) = (phase.sin(), phase.cos());
            let (dsin, dcos) = (step.sin(), step.cos());
            for (i, sample) in out.iter_mut().enumerate().take(i1).skip(i0) {
                let dt = i as f64 / rate as f64 - local_start;
                if dt < 0.0 {
                    continue;
                }
                let envelope = (-dt * note.decay).exp();
                *sample += (amp * envelope * sin) as f32;
                let next_sin = sin * dcos + cos * dsin;
                cos = cos * dcos - sin * dsin;
                sin = next_sin;
            }
        }
    }
    out
}

/// Render the performance into a WAV file.
fn write_performance(path: &Path, notes: &[Note], rate: u32, offset: f64, duration: f64) {
    let samples = render(notes, rate, offset, duration);
    let mut writer = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create");
    writer.write_planar(&[samples]).expect("write");
    writer.finalize().expect("finalise");
}

// ---------------------------------------------------------------------------
// The log-mel reports the rate its frames actually sit at
// ---------------------------------------------------------------------------

/// A file's log-mel must report the rate its frames actually sit at.
///
/// The hop is rounded to whole samples, so the nominal rate is only the actual
/// rate when the sample rate divides by the hop exactly. A 44.1 kHz file's
/// frames are 32.0017 ms apart; a report that said 32 ms would be wrong by 143
/// ppm, which is 690 ms over an eighty-minute recording.
#[test]
fn a_log_mel_reports_the_rate_its_frames_actually_sit_at() {
    let dir = TempDir::new().expect("temp dir");
    let notes = build_notes(8);

    for rate in [44100u32, 48000] {
        let path = dir.path().join(format!("tone-{rate}.wav"));
        write_performance(&path, &notes, rate, 0.0, 4.0);

        let mut decoder = AudioDecoder::open(&path).expect("open");
        let mel = extract_log_mel(&mut decoder, LogMelConfig::default(), 4096, None)
            .expect("extract the log-mel");

        assert!(
            (mel.rate_hz - expected_mel_rate(rate)).abs() < 1e-9,
            "a {rate} Hz file's log-mel claims {} frames per second, not {}",
            mel.rate_hz,
            expected_mel_rate(rate)
        );
        // The nominal rate is 1000 / hop_ms. It is right only when the sample
        // rate divides by the hop exactly.
        let nominal = 1000.0 / 32.0;
        if rate == 48000 {
            assert!((mel.rate_hz - nominal).abs() < 1e-9);
        } else {
            assert!(
                (mel.rate_hz - nominal).abs() > 1e-5,
                "a {rate} Hz file's log-mel still claims the nominal rate"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The frame arithmetic
// ---------------------------------------------------------------------------

/// A deterministic sequence with no repeating structure.
fn pseudo_random(n: usize, seed: u64) -> Vec<f32> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as f64 / (1u64 << 31) as f64 * 2.0 - 1.0) as f32
        })
        .collect()
}

/// A one-band log-mel with the given rate and frames.
fn one_band(rate_hz: f64, frames: &[f32]) -> LogMel {
    LogMel {
        rate_hz,
        n_bands: 1,
        n_frames: frames.len(),
        values: frames.to_vec(),
    }
}

/// The cross-check must do its frame arithmetic in each file's own frame grid.
///
/// The log-mel hop is rounded to whole samples, so two files whose sample rates
/// differ have frames a different distance apart: a 44.1 kHz file's frames are
/// 32.0017 ms apart and a 48 kHz file's are 32 ms apart. The cross-check
/// reports a source time and a reference time, and both have to be exact frame
/// times in the file they belong to.
///
/// The log-mel is built by hand here, so every frame index is known exactly and
/// the expected answer is arithmetic rather than a measurement. The two cases
/// are mirrors of each other: the first puts the 48 kHz file in the source and
/// the 44.1 kHz file in the reference, the second the other way round. The
/// first catches a reference time computed with the wrong rate; the second
/// catches a source time computed with the wrong rate and a probe that walks
/// the source's frames by the wrong step.
#[test]
fn a_spectral_cross_check_works_in_each_files_own_frame_grid() {
    // 48 kHz: the mel hop is 1536 samples, so the frames are exactly 32 ms apart.
    // 44.1 kHz: the hop is 1411 samples, so the frames are 32.0017 ms apart.
    const RATE_48: f64 = 48000.0 / 1536.0;
    const RATE_441: f64 = 44100.0 / 1411.0;
    /// How many frames later the reference's copy of the performance sits.
    const SHIFT: usize = 4000;
    const SOURCE_FRAMES: usize = 13000;
    const REFERENCE_FRAMES: usize = SHIFT + SOURCE_FRAMES + 100;

    let source_seq = pseudo_random(SOURCE_FRAMES, 0x5EED_0001);
    let mut reference_seq = vec![0.0f32; REFERENCE_FRAMES];
    reference_seq[SHIFT..SHIFT + SOURCE_FRAMES].copy_from_slice(&source_seq);

    let options = FineOptions {
        window_secs: 10.0,
        hop_secs: 10.0,
        search_secs: 3.0,
        ..FineOptions::default()
    };

    for (source_rate, reference_rate) in [(RATE_48, RATE_441), (RATE_441, RATE_48)] {
        let source = one_band(source_rate, &source_seq);
        let reference = one_band(reference_rate, &reference_seq);
        let model = LinearModel::new(1.0, SHIFT as f64 / reference_rate);

        let points = spectral_offsets(&source, &reference, &model, &options);
        assert!(
            points.len() >= 20,
            "the cross-check only produced {} points",
            points.len()
        );

        for point in &points {
            // The source time has to be an exact frame time of the source.
            let frame = point.source_time * source_rate;
            assert!(
                (frame - frame.round()).abs() < 1e-6,
                "the source time {frame} is not a whole number of {source_rate} Hz frames"
            );
            // And the reference time has to be the reference's frame that carries
            // the same material.
            let expected = (frame.round() as usize + SHIFT) as f64 / reference_rate;
            assert!(
                (point.reference_time - expected).abs() < 0.005,
                "the reference time is {:.4} s instead of {expected:.4} s",
                point.reference_time
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The cross-check finds the correspondence it is given
// ---------------------------------------------------------------------------

/// The spectral cross-check must find the correspondence it is given, at both
/// rates, to a few milliseconds.
///
/// The source and the reference are the same performance sampled at 44.1 and
/// 48 kHz, and the source starts ten seconds into the reference, so the
/// correspondence is exactly `reference_time = source_time + 10`. The cross
/// check's answer is compared against that, not against the envelope: the
/// envelope is the thing that was wrong before, and a test that only compared
/// the two estimators could not tell a shared mistake from agreement.
#[test]
fn a_spectral_cross_check_finds_the_correspondence_at_44_1_and_48_kilohertz() {
    let dir = TempDir::new().expect("temp dir");

    const SOURCE_DURATION: f64 = 120.0;
    const OFFSET: f64 = 10.0;

    let notes = build_notes(220);
    let reference_path = dir.path().join("reference.wav");
    write_performance(
        &reference_path,
        &notes,
        48000,
        0.0,
        SOURCE_DURATION + OFFSET + 5.0,
    );
    let source_path = dir.path().join("source.wav");
    write_performance(&source_path, &notes, 44100, OFFSET, SOURCE_DURATION);

    let model = LinearModel::new(1.0, OFFSET);
    let options = FineOptions::default();

    let mut reference_decoder = AudioDecoder::open(&reference_path).expect("open");
    let reference_mel =
        extract_log_mel(&mut reference_decoder, LogMelConfig::default(), 4096, None)
            .expect("extract the reference log-mel");
    let mut source_decoder = AudioDecoder::open(&source_path).expect("open");
    let source_mel = extract_log_mel(&mut source_decoder, LogMelConfig::default(), 4096, None)
        .expect("extract the source log-mel");

    let points = spectral_offsets(&source_mel, &reference_mel, &model, &options);
    assert!(
        points.len() >= 6,
        "the cross-check only produced {} points",
        points.len()
    );

    // Every probe must land on the correspondence.
    //
    // The tolerance is the cross-check's own resolution on this material: the
    // log-mel correlation places the peak to within a fraction of a frame, and
    // the peak of a thirty-second window is a fraction of a frame wide. The
    // **rate** arithmetic is pinned down exactly by the frame-arithmetic test
    // above, which knows every frame index; this test checks that the whole
    // path — the extraction, the correlation and the conversion — still finds
    // the correspondence the two files actually have.
    let mut worst = 0.0f64;
    for point in &points {
        let expected = point.source_time + OFFSET;
        let error = (point.reference_time - expected).abs() * 1000.0;
        worst = worst.max(error);
        assert!(
            error < 30.0,
            "the cross-check put the correspondence at {:.4} s instead of {:.4} s ({error:.1} ms off)",
            point.reference_time,
            expected
        );
    }
    assert!(worst < 30.0, "the worst probe was {worst:.1} ms off");
}

// ---------------------------------------------------------------------------
// The cross-check still disagrees when the two really do disagree
// ---------------------------------------------------------------------------

/// The cross-check must still find a disagreement when the reference really is
/// a different performance.
///
/// A cross-check that always agreed would be worthless: it would have to be
/// agreeing for the wrong reason. Here the reference is displaced by a tenth
/// of a second relative to the source after the first minute — twice the fifty
/// milliseconds the pipeline treats as a disagreement — and the cross-check has
/// to see it.
#[test]
fn a_spectral_cross_check_still_sees_a_real_disagreement() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 90.0;
    const SLIP: f64 = 0.1;

    let notes = build_notes(200);
    let reference_path = dir.path().join("reference.wav");
    write_performance(&reference_path, &notes, 48000, 0.0, DURATION + 1.0);
    let source_path = dir.path().join("source.wav");
    write_performance(&source_path, &notes, 44100, 0.0, DURATION);

    let model = LinearModel::new(1.0, 0.0);
    let options = FineOptions::default();

    let mut reference_decoder = AudioDecoder::open(&reference_path).expect("open");
    let reference_mel =
        extract_log_mel(&mut reference_decoder, LogMelConfig::default(), 4096, None)
            .expect("extract the reference log-mel");
    let mut source_decoder = AudioDecoder::open(&source_path).expect("open");
    let source_mel = extract_log_mel(&mut source_decoder, LogMelConfig::default(), 4096, None)
        .expect("extract the source log-mel");

    let points = spectral_offsets(&source_mel, &reference_mel, &model, &options);
    assert!(!points.is_empty(), "the cross-check produced no points");

    // Now build a second reference in which the performance sits a tenth of a
    // second later, and check that the cross-check follows it rather than
    // reporting the old position.
    let mut slipped_notes = build_notes(200);
    for note in &mut slipped_notes {
        note.start += SLIP;
    }
    let slipped_path = dir.path().join("slipped.wav");
    write_performance(&slipped_path, &slipped_notes, 48000, 0.0, DURATION + 1.0);

    let mut slipped_decoder = AudioDecoder::open(&slipped_path).expect("open");
    let slipped_mel = extract_log_mel(&mut slipped_decoder, LogMelConfig::default(), 4096, None)
        .expect("extract the slipped log-mel");
    let slipped_points = spectral_offsets(&source_mel, &slipped_mel, &model, &options);
    assert!(
        !slipped_points.is_empty(),
        "the cross-check produced no points"
    );

    // The two references differ by a tenth of a second, which is twice the
    // fifty-millisecond threshold the pipeline warns at. The cross-check has to
    // report that difference, and it has to report it to the right size.
    let mut compared = 0usize;
    let mut largest = 0.0f64;
    let mut differences: Vec<f64> = Vec::new();
    for point in &points {
        let Some(other) = slipped_points
            .iter()
            .filter(|o| (o.source_time - point.source_time).abs() < 1.0)
            .min_by(|a, b| {
                (a.source_time - point.source_time)
                    .abs()
                    .partial_cmp(&(b.source_time - point.source_time).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        let difference = (other.reference_time - point.reference_time).abs() * 1000.0;
        largest = largest.max(difference);
        differences.push(difference);
        compared += 1;
    }
    assert!(compared >= 3, "only {compared} positions could be compared");
    assert!(
        largest > 50.0,
        "the cross-check did not see a 100 ms slide: the largest difference was {largest:.1} ms"
    );
    // And it has to have measured the slide, not merely noticed that something
    // changed.
    let median = {
        let mut sorted = differences.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    };
    assert!(
        (median - 100.0).abs() < 40.0,
        "the cross-check measured the slide as {median:.1} ms instead of 100 ms"
    );
}

// ---------------------------------------------------------------------------
// The envelope fit is untouched
// ---------------------------------------------------------------------------

/// The envelope fit must be the same after the change: it is the estimator the
/// drift is measured with, and the cross-check is only ever a second opinion.
///
/// A change to the cross-check must not quietly move the model.
#[test]
fn the_envelope_fit_is_unchanged_by_the_cross_check_fix() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 120.0;
    const OFFSET: f64 = 7.5;

    let notes = build_notes(220);
    let reference_path = dir.path().join("reference.wav");
    write_performance(&reference_path, &notes, 48000, 0.0, DURATION + OFFSET + 5.0);
    let source_path = dir.path().join("source.wav");
    write_performance(&source_path, &notes, 44100, OFFSET, DURATION);

    let mut reference_decoder = AudioDecoder::open(&reference_path).expect("open");
    let reference_env = extract_envelope(
        &mut reference_decoder,
        EnvelopeConfig::default(),
        4096,
        None,
    )
    .expect("extract the reference envelope");
    let mut source_decoder = AudioDecoder::open(&source_path).expect("open");
    let source_env = extract_envelope(&mut source_decoder, EnvelopeConfig::default(), 4096, None)
        .expect("extract the source envelope");

    let model = LinearModel::new(1.0, OFFSET);
    let fine = fine_match(&source_env, &reference_env, &model, &FineOptions::default());
    assert!(
        !fine.points.is_empty(),
        "the envelope fit produced no points"
    );

    for point in &fine.points {
        let expected = point.source_time + OFFSET;
        let error = (point.reference_time - expected).abs() * 1000.0;
        assert!(
            error < 10.0,
            "the envelope put the correspondence at {:.4} s instead of {:.4} s ({error:.1} ms off)",
            point.reference_time,
            expected
        );
    }
}
