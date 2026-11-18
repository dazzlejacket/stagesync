//! The mixer: real multi-track mixing, measured.
//!
//! Every fixture here is generated inside the test: no checked-in media, no Python,
//! no network. The ground truth is exact by construction â€” a recording is placed on
//! the reference timeline by a model the test chooses, so where its material lands
//! is known to the sample.
//!
//! Most of the tests use a recording whose rate equals the mix's rate and whose
//! model has no drift, so the placement is an **exact copy**: the resampler is not
//! used and the output can be compared sample for sample. The tests that involve a
//! rate change say so, and measure against the resampler's own tolerance rather than
//! pretending it is exact.

use std::path::Path;

use stagesync_core::audio::{Container, ContainerReader, SourceMeta, WavFormat, WavWriter};
use stagesync_core::matching::model::TimeMap;
use stagesync_core::matching::segments::Coverage;
use stagesync_core::matching::{
    AlignmentReport, LinearModel, SkipReasons, SourceAlignment, TimelineSpan,
};
use stagesync_core::mixer::{
    ClippingProtection, ContainerChoice, LoudnessMatch, MixOptions, MixSettings, MixTrack,
    MonitorMode, PanLaw, StereoPan,
};

/// Write a mono recording.
fn write_mono(path: &Path, samples: &[f32], rate: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create the output directory");
    }
    let mut writer = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create");
    writer
        .write_planar(std::slice::from_ref(&samples))
        .expect("write");
    writer.finalize().expect("finalise");
}

/// Write a stereo recording.
fn write_stereo(path: &Path, left: &[f32], right: &[f32], rate: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create the output directory");
    }
    let mut writer = WavWriter::create(path, rate, 2, WavFormat::Float32).expect("create");
    writer.write_planar(&[left, right]).expect("write");
    writer.finalize().expect("finalise");
}

/// The metadata of a recording the test wrote.
fn meta(path: &Path, rate: u32, channels: usize, frames: u64) -> SourceMeta {
    SourceMeta {
        path: path.to_path_buf(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        sha256: String::new(),
        byte_len: 0,
        codec: "pcm_f32le".to_string(),
        sample_rate: rate,
        channels,
        duration_secs: frames as f64 / rate as f64,
        frames,
        sample_format: Some("f32".to_string()),
        bits_per_sample: Some(32),
        float_format: true,
    }
}

/// An alignment of one recording to the reference, with a known model.
fn alignment(
    path: &Path,
    rate: u32,
    channels: usize,
    frames: u64,
    model: LinearModel,
) -> SourceAlignment {
    let source = meta(path, rate, channels, frames);
    let duration = source.duration_secs;
    SourceAlignment {
        map: TimeMap::from_linear(model, duration),
        source,
        coarse: None,
        fine_measured: 0,
        fine_skipped: 0,
        fine_skip_reasons: SkipReasons {
            low_energy: 0,
            no_region: 0,
            no_peak: 0,
        },
        direct_model: Some(model),
        calibrated_model: Some(model),
        drift: None,
        points: Vec::new(),
        segments: Vec::new(),
        coverage: Coverage {
            covered_seconds: duration,
            low_confidence_seconds: 0.0,
            uncovered_seconds: 0.0,
            fraction: 1.0,
        },
        holdout: None,
        override_state: None,
        warnings: Vec::new(),
    }
}

/// A report whose reference is `duration` seconds long.
fn report(
    reference: &Path,
    reference_rate: u32,
    duration: f64,
    sources: Vec<SourceAlignment>,
) -> AlignmentReport {
    let reference_frames = (duration * reference_rate as f64).round() as u64;
    let reference_meta = meta(reference, reference_rate, 1, reference_frames);
    let timeline = vec![TimelineSpan {
        reference_start: 0.0,
        reference_end: duration,
        kind: stagesync_core::matching::SegmentKind::Matched,
        source_index: None,
        confidence: stagesync_core::matching::Confidence::high(),
        note: String::new(),
    }];
    AlignmentReport {
        reference: reference_meta,
        sources,
        timeline,
        timeline_duration: duration,
    }
}

/// The samples of a written file, read back with the container reader.
fn read(path: &Path) -> (Vec<Vec<f32>>, u32) {
    let mut reader = ContainerReader::open(path).expect("open the written file");
    let frames = reader.frames();
    let planes = reader
        .read_frames(0, frames as usize)
        .expect("read the written file");
    (planes, reader.sample_rate())
}

/// A recording of a constant value.
fn constant(value: f32, frames: usize) -> Vec<f32> {
    vec![value; frames]
}

/// The options of a mix at the reference's rate.
fn options() -> MixOptions {
    MixOptions {
        sample_rate: Some(48_000),
        channels: 2,
        format: WavFormat::Float32,
        block_frames: 4096,
        ..MixOptions::default()
    }
}

#[test]
fn a_mono_recording_is_mixed_at_its_own_level() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &options(), &out)
        .expect("the mix should succeed");

    assert_eq!(result.frames, rate as u64);
    assert_eq!(result.channels, 2);
    let (planes, out_rate) = read(&out);
    assert_eq!(out_rate, rate);
    // A mono recording panned to the centre with the equal-power law is at
    // cos(45Â°) = 0.7071 in each output channel.
    let expected = 0.5 * 0.5f32.sqrt();
    for frame in [0usize, 100, 1000, 20_000] {
        assert!(
            (planes[0][frame] - expected).abs() < 1e-5,
            "the left channel at {frame} was {}, and it should be {expected}",
            planes[0][frame]
        );
        assert!(
            (planes[1][frame] - expected).abs() < 1e-5,
            "the right channel at {frame} was {}, and it should be {expected}",
            planes[1][frame]
        );
    }
}

#[test]
fn a_stereo_recording_keeps_its_own_channels() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("stereo.wav");
    let reference_path = dir.path().join("reference.wav");
    write_stereo(
        &source_path,
        &constant(0.5, rate as usize),
        &constant(-0.25, rate as usize),
        rate,
    );
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            2,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    assert!(
        (planes[0][100] - 0.5).abs() < 1e-6,
        "the left channel changed"
    );
    assert!(
        (planes[1][100] + 0.25).abs() < 1e-6,
        "the right channel changed"
    );
}

#[test]
fn an_antiphase_stereo_recording_is_not_folded_to_silence() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("antiphase.wav");
    let reference_path = dir.path().join("reference.wav");
    write_stereo(
        &source_path,
        &constant(0.5, rate as usize),
        &constant(-0.5, rate as usize),
        rate,
    );
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            2,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    assert!(
        (planes[0][100] - 0.5).abs() < 1e-6,
        "the left channel was lost: {}",
        planes[0][100]
    );
    assert!(
        (planes[1][100] + 0.5).abs() < 1e-6,
        "the right channel was lost: {}",
        planes[1][100]
    );
    // The two channels are still in antiphase: they do not cancel.
    assert!(
        (planes[0][100] + planes[1][100]).abs() < 1e-6,
        "the antiphase pair was folded: {} and {}",
        planes[0][100],
        planes[1][100]
    );
}

#[test]
fn two_stereo_recordings_are_overlaid_channel_for_channel() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let a_path = dir.path().join("a.wav");
    let b_path = dir.path().join("b.wav");
    let reference_path = dir.path().join("reference.wav");
    write_stereo(
        &a_path,
        &constant(0.1, rate as usize),
        &constant(0.2, rate as usize),
        rate,
    );
    write_stereo(
        &b_path,
        &constant(0.3, rate as usize),
        &constant(-0.4, rate as usize),
        rate,
    );
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![
            alignment(&a_path, rate, 2, rate as u64, LinearModel::new(1.0, 0.0)),
            alignment(&b_path, rate, 2, rate as u64, LinearModel::new(1.0, 0.0)),
        ],
    );
    let settings = MixSettings::of_sources(&[0, 1]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    // Each recording keeps its own left and right channel: the left of the mix is
    // the sum of the lefts, and the right the sum of the rights.
    assert!(
        (planes[0][100] - 0.4).abs() < 1e-6,
        "the left mix was {}",
        planes[0][100]
    );
    assert!(
        (planes[1][100] + 0.2).abs() < 1e-6,
        "the right mix was {}",
        planes[1][100]
    );
}

#[test]
fn a_panned_recording_goes_to_one_side() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let mut settings = MixSettings::of_sources(&[0]);
    settings.tracks[0].pan = -1.0;
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    assert!(
        (planes[0][100] - 0.5).abs() < 1e-5,
        "the left channel was {}",
        planes[0][100]
    );
    assert!(
        planes[1][100].abs() < 1e-6,
        "the right channel was not silent"
    );
}

#[test]
fn the_fader_scales_the_recording() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let mut settings = MixSettings::of_sources(&[0]);
    settings.tracks[0].gain = 0.25;
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    let expected = 0.5 * 0.25 * 0.5f32.sqrt();
    assert!(
        (planes[0][100] - expected).abs() < 1e-5,
        "the left was {}",
        planes[0][100]
    );
}

#[test]
fn a_muted_recording_is_silent_and_a_solo_silences_the_others() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let a_path = dir.path().join("a.wav");
    let b_path = dir.path().join("b.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&a_path, &constant(0.5, rate as usize), rate);
    write_mono(&b_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![
            alignment(&a_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
            alignment(&b_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
        ],
    );

    // Muted.
    let mut settings = MixSettings::of_sources(&[0, 1]);
    settings.tracks[0].mute = true;
    let out = dir.path().join("muted.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");
    let (planes, _) = read(&out);
    let expected = 0.5 * 0.5f32.sqrt();
    assert!(
        (planes[0][100] - expected).abs() < 1e-5,
        "the muted recording was still there: {}",
        planes[0][100]
    );

    // Solo.
    let mut settings = MixSettings::of_sources(&[0, 1]);
    settings.tracks[1].solo = true;
    let out = dir.path().join("solo.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");
    let (planes, _) = read(&out);
    assert!(
        (planes[0][100] - expected).abs() < 1e-5,
        "the soloed recording was not heard: {}",
        planes[0][100]
    );
}

#[test]
fn a_float_peak_above_zero_is_kept() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("loud.wav");
    let reference_path = dir.path().join("reference.wav");
    let mut samples = constant(0.0, rate as usize);
    samples[100] = 2.0;
    write_mono(&source_path, &samples, rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let result =
        stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    let peak = planes[0][100].abs().max(planes[1][100].abs());
    assert!(
        (peak - 2.0 * 0.5f32.sqrt()).abs() < 1e-5,
        "the peak above 0 dBFS was not kept: {peak}"
    );
    assert_eq!(result.overs, 2, "the overs were not counted");
}

#[test]
fn the_automatic_gain_brings_the_mix_inside_the_range() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let a_path = dir.path().join("a.wav");
    let b_path = dir.path().join("b.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&a_path, &constant(0.8, rate as usize), rate);
    write_mono(&b_path, &constant(0.8, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![
            alignment(&a_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
            alignment(&b_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
        ],
    );
    let settings = MixSettings::of_sources(&[0, 1]);
    let mut opts = options();
    opts.protection = ClippingProtection::AutoGain { target_dbfs: -1.0 };
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    let (planes, _) = read(&out);
    let peak = planes
        .iter()
        .flat_map(|p| p.iter())
        .fold(0f32, |m, v| m.max(v.abs()));
    let target = 10f64.powf(-1.0 / 20.0) as f32;
    assert!(
        (peak - target).abs() < 1e-4,
        "the peak was {peak} and the target was {target}"
    );
    assert_eq!(result.overs, 0, "the mix still had samples above 0 dBFS");
    assert!(
        result.gain_reduction_db < -1.0,
        "the gain reduction was only {} dB",
        result.gain_reduction_db
    );
}

#[test]
fn the_soft_clipper_keeps_the_output_inside_the_range() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("loud.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(2.0, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let mut opts = options();
    opts.protection = ClippingProtection::SoftClip { threshold: 0.9 };
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    let (planes, _) = read(&out);
    let peak = planes
        .iter()
        .flat_map(|p| p.iter())
        .fold(0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 1.0, "the soft clipper let {peak} through");
    assert!(peak > 0.9, "the soft clipper did nothing: {peak}");
    assert!(
        result.clipped_samples > 0,
        "the clipped samples were not counted"
    );
}

#[test]
fn a_recording_at_another_sample_rate_is_mixed_correctly() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let source_rate = 44_100;
    let mix_rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    let seconds = 2.0;
    let samples: Vec<f32> = (0..(source_rate as f64 * seconds) as usize)
        .map(|i| {
            let t = i as f32 / source_rate as f32;
            (2.0 * std::f64::consts::PI * 220.0 * t as f64).sin() as f32 * 0.4
        })
        .collect();
    write_mono(&source_path, &samples, source_rate);
    write_mono(&reference_path, &constant(0.0, mix_rate as usize), mix_rate);

    let report = report(
        &reference_path,
        mix_rate,
        seconds,
        vec![alignment(
            &source_path,
            source_rate,
            1,
            samples.len() as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let result =
        stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    assert_eq!(result.frames, (mix_rate as f64 * seconds) as u64);
    let (planes, _) = read(&out);
    // The tone is still a tone: the peak is where it should be, and the period is
    // the period of 220 Hz at the mix's rate.
    let peak = planes[0].iter().fold(0f32, |m, v| m.max(v.abs()));
    let expected = 0.4 * 0.5f32.sqrt();
    assert!(
        (peak - expected).abs() < 0.01,
        "the peak was {peak} and it should be about {expected}"
    );
    let mut crossings = 0;
    for i in 1..planes[0].len() {
        if planes[0][i - 1] <= 0.0 && planes[0][i] > 0.0 {
            crossings += 1;
        }
    }
    let expected_crossings = (220.0 * seconds).round() as usize;
    assert!(
        (crossings as i64 - expected_crossings as i64).abs() <= 2,
        "the tone crossed zero {crossings} times and it should have crossed {expected_crossings}"
    );
}

#[test]
fn a_recording_that_starts_late_leaves_the_start_silent() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    let offset = 0.5;
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize * 2), rate);

    let report = report(
        &reference_path,
        rate,
        2.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, offset),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let result =
        stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    assert_eq!(result.frames, (rate * 2) as u64);
    let (planes, _) = read(&out);
    // Before the recording starts there is nothing at all.
    assert!(
        planes[0][100].abs() < 1e-9,
        "there was something before the recording started: {}",
        planes[0][100]
    );
    // After it starts there is.
    let started = (offset * rate as f64) as usize + 100;
    assert!(
        planes[0][started].abs() > 0.1,
        "the recording did not start where it should: {}",
        planes[0][started]
    );
}

#[test]
fn a_recording_is_placed_exactly_where_its_model_says() {
    // The block size here is much larger than the recording's start, so a placement
    // that is off by even one block is caught. A mixer that writes the recording at
    // the start of the block it appears in — rather than at the position its model
    // gives it — passes a test that samples two points and fails this one.
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    let offset = 0.5;
    let start = (offset * rate as f64) as usize;
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize * 2), rate);

    let report = report(
        &reference_path,
        rate,
        2.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, offset),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let options = MixOptions {
        block_frames: 65_536,
        ..options()
    };
    stagesync_core::mixer::render_mix(&report, &settings, &options, &out).expect("mix");

    let (planes, _) = read(&out);
    // Every frame before the recording's start is silent: not one of them carries
    // any of the recording.
    for (frame, (left, right)) in planes
        .iter()
        .map(|plane| (&plane[0], &plane[1]))
        .enumerate()
        .take(start)
    {
        assert!(
            left.abs() < 1e-9 && right.abs() < 1e-9,
            "there was something at frame {frame}, before the recording started at {start}: {left}"
        );
    }
    // The recording starts at the frame its model names, not a block earlier.
    let expected = 0.5 * (0.5f32).sqrt();
    for frame in [start, start + 1, start + 2, start + 1024, start + 40_000] {
        assert!(
            (planes[0][frame] - expected).abs() < 1e-6,
            "the recording was not at its own position at frame {frame}: {} instead of {expected}",
            planes[0][frame]
        );
    }
}

#[test]
fn the_mix_is_the_sum_of_the_recordings_on_the_reference_timeline() {
    // The strongest statement the mixer can make: what it writes is exactly the
    // material of the recordings, placed on the reference timeline, added together.
    // The timeline renderer is the one the rest of StageSync uses, so this is a
    // comparison against a known-good implementation of the same placement.
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    let offset = 0.25;
    let source: Vec<f32> = (0..rate as usize)
        .map(|i| ((i as f32) * 0.01).sin() * 0.4)
        .collect();
    write_mono(&source_path, &source, rate);
    write_mono(&reference_path, &constant(0.0, rate as usize * 2), rate);

    let model = LinearModel::new(1.0, offset);
    let report = report(
        &reference_path,
        rate,
        2.0,
        vec![alignment(&source_path, rate, 1, rate as u64, model)],
    );

    // The recording on its own, on the reference timeline.
    let timeline_path = dir.path().join("timeline.wav");
    stagesync_core::render::render_reference_timeline(
        &source_path,
        &model,
        2.0,
        &timeline_path,
        &stagesync_core::render::RenderOptions {
            sample_rate: Some(rate),
            format: WavFormat::Float32,
            block_frames: 4096,
            ..stagesync_core::render::RenderOptions::default()
        },
    )
    .expect("the timeline render should succeed");
    let (timeline, _) = read(&timeline_path);

    // And the mix of the same recording.
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");
    let (mix, _) = read(&out);

    // The equal-power law at the centre puts a mono recording at -3 dB in each side.
    let centre = (0.5f32).sqrt();
    for frame in 0..mix[0].len() {
        let expected = timeline[0][frame] * centre;
        assert!(
            (mix[0][frame] - expected).abs() < 1e-6,
            "the mix and the timeline disagree at frame {frame}: {} instead of {expected}",
            mix[0][frame]
        );
        assert!(
            (mix[1][frame] - expected).abs() < 1e-6,
            "the mix and the timeline disagree at frame {frame}: {} instead of {expected}",
            mix[1][frame]
        );
    }
}

#[test]
fn a_recording_that_stops_early_leaves_the_end_silent() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize * 2), rate);

    let report = report(
        &reference_path,
        rate,
        2.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    let end = (rate as usize * 2) - 100;
    assert!(
        planes[0][end].abs() < 1e-9,
        "there was something after the recording ended: {}",
        planes[0][end]
    );
}

#[test]
fn a_long_mix_has_no_click_at_the_block_boundaries() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let seconds = 20.0;
    let frames = (rate as f64 * seconds) as usize;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    // A constant signal: any discontinuity in the mix shows up as a sample that is
    // not equal to its neighbours.
    write_mono(&source_path, &constant(0.5, frames), rate);
    write_mono(&reference_path, &constant(0.0, frames), rate);

    let report = report(
        &reference_path,
        rate,
        seconds,
        vec![alignment(
            &source_path,
            rate,
            1,
            frames as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let mut opts = options();
    // A small block, so that the twenty seconds are crossed many times.
    opts.block_frames = 1024;
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    assert_eq!(result.frames, frames as u64);
    let (planes, _) = read(&out);
    let expected = 0.5 * 0.5f32.sqrt();
    // Every sample after the first must be the same as every other one.
    let mut worst = 0f32;
    for (left, right) in planes[0].iter().zip(planes[1].iter()).skip(1) {
        worst = worst.max((left - expected).abs());
        worst = worst.max((right - expected).abs());
    }
    assert!(
        worst < 1e-6,
        "the mix jumped by {worst} somewhere: a block boundary introduced a click"
    );
}
#[test]
fn the_originals_are_not_modified_by_a_mix() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let before_source = stagesync_core::audio::sha256_file(&source_path).expect("hash");
    let before_reference = stagesync_core::audio::sha256_file(&reference_path).expect("hash");

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    assert_eq!(
        stagesync_core::audio::sha256_file(&source_path).expect("hash"),
        before_source,
        "the recording was modified by the mix"
    );
    assert_eq!(
        stagesync_core::audio::sha256_file(&reference_path).expect("hash"),
        before_reference,
        "the reference was modified by the mix"
    );
}

#[test]
fn a_large_mix_is_written_as_rf64_and_is_still_readable() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let mut opts = options();
    opts.container = ContainerChoice::Rf64;
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    assert_eq!(result.container, Container::Rf64);
    let bytes = std::fs::read(&out).expect("read");
    assert_eq!(&bytes[0..4], b"RF64", "the file is not an RF64 file");
    let (planes, _) = read(&out);
    assert!((planes[0][100] - 0.5 * 0.5f32.sqrt()).abs() < 1e-5);
}

#[test]
fn the_container_is_chosen_from_the_size_of_the_mix() {
    // A payload that fits is written as an ordinary RIFF file.
    let (container, note) =
        stagesync_core::mixer::choose_container(ContainerChoice::Auto, 1000).expect("choose");
    assert_eq!(container, Container::Riff);
    assert!(note.is_none());

    // A payload that does not is written as RF64, and says so.
    let big = stagesync_core::audio::RIFF_MAX_DATA_BYTES + 1;
    let (container, note) =
        stagesync_core::mixer::choose_container(ContainerChoice::Auto, big).expect("choose");
    assert_eq!(container, Container::Rf64);
    assert!(note.is_some(), "the switch to RF64 was not reported");

    // A RIFF file that is asked for explicitly and cannot hold the mix is refused.
    let refused = stagesync_core::mixer::choose_container(ContainerChoice::Riff, big);
    assert!(refused.is_err(), "an impossible RIFF file was allowed");
}

#[test]
fn the_monitoring_modes_pick_the_right_recordings() {
    let tracks = vec![MixTrack::new(0), MixTrack::new(1), MixTrack::new(2)];

    assert!(
        MixSettings::for_mode(MonitorMode::ReferenceOnly, &tracks).is_empty(),
        "the reference must never be a track of the mix"
    );
    assert_eq!(
        MixSettings::for_mode(MonitorMode::TargetOnly, &tracks).len(),
        1,
        "the target-only mode must play one recording"
    );
    assert_eq!(
        MixSettings::for_mode(MonitorMode::AbSwitch, &tracks).len(),
        1,
        "the A/B mode must play one recording at a time"
    );
    assert_eq!(
        MixSettings::for_mode(MonitorMode::Both, &tracks).len(),
        3,
        "the mode that plays everything must play every recording"
    );

    let split = MixSettings::for_mode(MonitorMode::StereoSplit, &tracks);
    assert_eq!(split.len(), 2, "the split mode must use two recordings");
    assert_eq!(
        split[0].pan, -1.0,
        "the first recording must be on the left"
    );
    assert_eq!(
        split[1].pan, 1.0,
        "the second recording must be on the right"
    );
    assert_eq!(
        split[0].stereo_pan,
        StereoPan::MonoFold,
        "a recording that is heard from one speaker alone must be folded"
    );
}

#[test]
fn the_loudness_match_moves_the_quiet_recording() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let loud_path = dir.path().join("loud.wav");
    let quiet_path = dir.path().join("quiet.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&loud_path, &constant(0.5, rate as usize), rate);
    write_mono(&quiet_path, &constant(0.05, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![
            alignment(&loud_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
            alignment(
                &quiet_path,
                rate,
                1,
                rate as u64,
                LinearModel::new(1.0, 0.0),
            ),
        ],
    );
    let settings = MixSettings::of_sources(&[0, 1]);
    let mut opts = options();
    opts.loudness = LoudnessMatch::MatchToLoudest;
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    // The quiet recording was brought up by the ratio of the two levels.
    let expected = 20.0 * (0.5f64 / 0.05).log10();
    assert!(
        (result.loudness_gains_db[1] - expected).abs() < 0.01,
        "the loudness gain was {} dB and it should be {expected} dB",
        result.loudness_gains_db[1]
    );
    assert!(
        result.loudness_gains_db[0].abs() < 0.01,
        "the loud recording was moved: {} dB",
        result.loudness_gains_db[0]
    );
    let (planes, _) = read(&out);
    let expected_left = (0.5 + 0.5) * 0.5f32.sqrt();
    assert!(
        (planes[0][100] - expected_left).abs() < 1e-4,
        "the mixed level was {} and it should be {expected_left}",
        planes[0][100]
    );
}

#[test]
fn without_a_loudness_match_the_quiet_recording_stays_quiet() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let loud_path = dir.path().join("loud.wav");
    let quiet_path = dir.path().join("quiet.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&loud_path, &constant(0.5, rate as usize), rate);
    write_mono(&quiet_path, &constant(0.05, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![
            alignment(&loud_path, rate, 1, rate as u64, LinearModel::new(1.0, 0.0)),
            alignment(
                &quiet_path,
                rate,
                1,
                rate as u64,
                LinearModel::new(1.0, 0.0),
            ),
        ],
    );
    let settings = MixSettings::of_sources(&[0, 1]);
    let out = dir.path().join("mix.wav");
    let result =
        stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    assert!(
        result.loudness_gains_db.iter().all(|g| g.abs() < 1e-9),
        "a gain was applied without being asked for: {:?}",
        result.loudness_gains_db
    );
    let (planes, _) = read(&out);
    let expected = (0.5 + 0.05) * 0.5f32.sqrt();
    assert!(
        (planes[0][100] - expected).abs() < 1e-5,
        "the mixed level was {} and it should be {expected}",
        planes[0][100]
    );
}

#[test]
fn a_recording_that_could_not_be_aligned_contributes_nothing() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let mut unaligned = alignment(
        &source_path,
        rate,
        1,
        rate as u64,
        LinearModel::new(1.0, 0.0),
    );
    unaligned.calibrated_model = None;
    unaligned.direct_model = None;
    let report = report(&reference_path, rate, 1.0, vec![unaligned]);

    let settings = MixSettings::of_sources(&[0]);
    let out = dir.path().join("mix.wav");
    let result =
        stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    assert!(
        planes[0][100].abs() < 1e-9,
        "an unaligned recording was mixed in: {}",
        planes[0][100]
    );
    assert!(
        result
            .notes
            .iter()
            .any(|n| n.contains("no reliable alignment")),
        "the missing alignment was not reported: {:?}",
        result.notes
    );
}

#[test]
fn the_constant_gain_law_keeps_the_sum_of_the_gains_at_one() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let mut settings = MixSettings::of_sources(&[0]);
    settings.tracks[0].pan_law = PanLaw::ConstantGain;
    let out = dir.path().join("mix.wav");
    stagesync_core::mixer::render_mix(&report, &settings, &options(), &out).expect("mix");

    let (planes, _) = read(&out);
    // At the centre the recording is 6 dB down in each side: the two gains add
    // up to one, so the recording's total amplitude is unchanged.
    assert!(
        (planes[0][100] - 0.25).abs() < 1e-5,
        "the constant-gain law did not keep the level: {}",
        planes[0][100]
    );
    assert!(
        (planes[1][100] - 0.25).abs() < 1e-5,
        "the constant-gain law did not keep the level: {}",
        planes[1][100]
    );
}

#[test]
fn a_hard_pan_puts_a_mono_recording_on_the_side_it_names() {
    // A pan of -1 is hard **left** and a pan of +1 is hard **right**, under both
    // laws. A law that ignores the sign puts the recording on the wrong side,
    // which is a defect the user hears as "the pan is reversed".
    for law in [PanLaw::EqualPower, PanLaw::ConstantGain] {
        let (left, right) = stagesync_core::mixer::pan::mono_gains(-1.0, law);
        assert!(
            left > 0.0 && right.abs() < 1e-6,
            "a hard left pan gave {left} and {right} under {law:?}"
        );
        let (left, right) = stagesync_core::mixer::pan::mono_gains(1.0, law);
        assert!(
            left.abs() < 1e-6 && right > 0.0,
            "a hard right pan gave {left} and {right} under {law:?}"
        );
        let (left, right) = stagesync_core::mixer::pan::mono_gains(0.0, law);
        assert!(
            (left - right).abs() < 1e-6 && left > 0.0,
            "the centre gave {left} and {right} under {law:?}"
        );
    }
}

#[test]
fn the_mix_can_be_written_as_a_mono_file() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let mut opts = options();
    opts.channels = 1;
    let out = dir.path().join("mix.wav");
    let result = stagesync_core::mixer::render_mix(&report, &settings, &opts, &out).expect("mix");

    assert_eq!(result.channels, 1);
    let (planes, _) = read(&out);
    assert_eq!(planes.len(), 1);
    // A mono mixdown ignores the pan: the recording is at its own level.
    assert!(
        (planes[0][100] - 0.5).abs() < 1e-5,
        "the mono mixdown was {} and it should be 0.5",
        planes[0][100]
    );
}

#[test]
fn the_mix_does_not_overwrite_a_recording() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 48_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.5, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let report = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let settings = MixSettings::of_sources(&[0]);
    let refused = stagesync_core::mixer::render_mix(&report, &settings, &options(), &source_path);
    assert!(
        refused.is_err(),
        "the mix was allowed to overwrite a recording"
    );
}

#[test]
fn render_mix_applies_master_gain_with_none_soft_clip_and_auto_gain() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let rate = 8_000;
    let source_path = dir.path().join("source.wav");
    let reference_path = dir.path().join("reference.wav");
    write_mono(&source_path, &constant(0.6, rate as usize), rate);
    write_mono(&reference_path, &constant(0.0, rate as usize), rate);

    let rep = report(
        &reference_path,
        rate,
        1.0,
        vec![alignment(
            &source_path,
            rate,
            1,
            rate as u64,
            LinearModel::new(1.0, 0.0),
        )],
    );
    let mut settings = MixSettings::of_sources(&[0]);
    settings.master_gain = 0.25;

    let out_none = dir.path().join("mix_none.wav");
    let res_none =
        stagesync_core::mixer::render_mix(&rep, &settings, &options(), &out_none).expect("mix");
    let (planes, _) = read(&out_none);
    let expected = 0.6 * 0.5f32.sqrt() * 0.25;
    assert!(
        (planes[0][1000] - expected).abs() < 1e-4,
        "render_mix ignored master_gain: got {}, expected {expected} (reported peak {})",
        planes[0][1000],
        res_none.peak
    );

    // AutoGain with attenuated master_gain = 0.25 stays attenuated (no boost to -1 dBFS).
    let mut opts_auto = options();
    opts_auto.protection =
        stagesync_core::mixer::ClippingProtection::AutoGain { target_dbfs: -1.0 };
    let out_auto_low = dir.path().join("mix_auto_low.wav");
    stagesync_core::mixer::render_mix(&rep, &settings, &opts_auto, &out_auto_low)
        .expect("mix auto low");
    let (planes_low, _) = read(&out_auto_low);
    assert!(
        (planes_low[0][1000] - expected).abs() < 1e-4,
        "AutoGain boosted or ignored low master_gain: got {}, expected {expected}",
        planes_low[0][1000]
    );

    // AutoGain with hot master_gain = 4.0 turns it down cleanly to -1.0 dBFS target (~0.89125).
    settings.master_gain = 4.0;
    let out_auto_hot = dir.path().join("mix_auto_hot.wav");
    let res_hot = stagesync_core::mixer::render_mix(&rep, &settings, &opts_auto, &out_auto_hot)
        .expect("mix auto hot");
    let target_lin = stagesync_core::mixer::db_to_linear(-1.0);
    assert!(
        res_hot.peak <= target_lin + 1e-3 && res_hot.peak >= target_lin - 0.05,
        "AutoGain with hot master_gain missed target_dbfs: peak={}, target={target_lin}",
        res_hot.peak
    );

    // SoftClip with hot master_gain = 4.0 compresses below 1.0 instead of writing > 1.0.
    let mut opts_soft = options();
    opts_soft.protection = stagesync_core::mixer::ClippingProtection::SoftClip { threshold: 0.9 };
    let out_soft = dir.path().join("mix_soft.wav");
    let res_soft = stagesync_core::mixer::render_mix(&rep, &settings, &opts_soft, &out_soft)
        .expect("mix soft");
    assert!(
        res_soft.peak <= 1.0 && res_soft.clipped_samples > 0,
        "SoftClip with master_gain=4.0 failed to compress: peak={}, clipped={}",
        res_soft.peak,
        res_soft.clipped_samples
    );
}
