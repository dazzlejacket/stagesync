//! Rendering tests: determinism, clipping, channel handling, gaps and immutability.

mod common;

use std::path::{Path, PathBuf};

use common::{
    analyse, config, make_source, reference_file, reference_signal, write_wav, write_wav_stereo,
    SourceSpec,
};
use stagesync_core::audio::{WavFormat, WavReader, WavWriter};
use stagesync_core::matching::{LinearModel, SegmentKind};
use stagesync_core::render::{
    render_merged_timeline, render_reference_timeline, render_source_aligned, Layout, RenderOptions,
};
use tempfile::TempDir;

const RATE: u32 = 8000;
const SOURCE_RATE: u32 = 7200;
const DURATION: f64 = 120.0;

/// Read a whole WAV file as mono samples.
fn read_mono(path: &Path) -> (Vec<f32>, u32, usize) {
    let mut reader = WavReader::open(path).expect("open the rendered file");
    let rate = reader.sample_rate();
    let channels = reader.channels();
    let frames = reader.frames() as usize;
    let planes = reader
        .read_frames(0, frames)
        .expect("read the rendered file");
    let mono: Vec<f32> = (0..frames)
        .map(|i| {
            planes
                .iter()
                .map(|p| p.get(i).copied().unwrap_or(0.0))
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    (mono, rate, channels)
}

/// Build a source file and return its path.
fn build_source(
    dir: &TempDir,
    name: &str,
    scale: f64,
    offset: f64,
    out_rate: u32,
    gain_db: f64,
    duration_secs: Option<f64>,
) -> PathBuf {
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
) -> PathBuf {
    let reference = reference_signal(RATE, reference_duration, 0xC0FFEE);
    let (samples, _) = make_source(&reference, RATE, spec, 0xBEEF);
    let path = dir.path().join(name);
    write_wav(&path, &samples, spec.out_rate, WavFormat::Float32);
    path
}

/// The model the analysis settled on for a source.
fn model_for(report: &stagesync_core::matching::AlignmentReport, index: usize) -> LinearModel {
    let alignment = &report.sources[index];
    alignment
        .calibrated_model
        .or(alignment.direct_model)
        .expect("the analysis should have produced a model")
}

#[test]
fn rendering_the_same_input_twice_produces_identical_bytes() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = build_source(
        &dir,
        "src.wav",
        1.0 + 35.0 / 1e6,
        25.0,
        SOURCE_RATE,
        -20.0,
        None,
    );
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);

    let options = RenderOptions::default();
    let first = dir.path().join("first.wav");
    let second = dir.path().join("second.wav");
    render_source_aligned(&source, &model, &first, &options).expect("first render");
    render_source_aligned(&source, &model, &second, &options).expect("second render");

    let a = std::fs::read(&first).expect("read the first render");
    let b = std::fs::read(&second).expect("read the second render");
    assert_eq!(a.len(), b.len(), "the two renders have different lengths");
    assert!(
        a == b,
        "two renders of the same input produced different bytes"
    );
}

#[test]
fn the_rendered_audio_contains_no_nan_and_does_not_clip() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = build_source(
        &dir,
        "src.wav",
        1.0 + 35.0 / 1e6,
        25.0,
        SOURCE_RATE,
        -20.0,
        None,
    );
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);

    let out = dir.path().join("corrected.wav");
    render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");
    let (samples, _, _) = read_mono(&out);

    assert!(!samples.is_empty(), "the render is empty");
    for (i, v) in samples.iter().enumerate() {
        assert!(v.is_finite(), "sample {i} is not finite: {v}");
    }
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 1.0, "the render peaks at {peak}, which would clip");
    assert!(
        peak > 0.05,
        "the render is essentially silent (peak {peak})"
    );
}

#[test]
fn a_stereo_source_keeps_its_channels_and_their_relationship() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (mono, _) = make_source(
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
    // The right channel is the left one at half the level: a known relationship that
    // the correction must preserve.
    let right: Vec<f32> = mono.iter().map(|v| v * 0.5).collect();
    let source = dir.path().join("stereo.wav");
    write_wav_stereo(&source, &mono, &right, SOURCE_RATE);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);
    let out = dir.path().join("corrected.wav");
    let result =
        render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");

    assert_eq!(result.channels, 2, "the channel count was not preserved");

    let mut reader = WavReader::open(&out).expect("open the render");
    assert_eq!(reader.channels(), 2, "the rendered file is not stereo");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read");
    let left = &planes[0];
    let right = &planes[1];

    // The right channel must still be half of the left one.
    let mut worst = 0.0f32;
    for i in 0..left.len().min(right.len()) {
        if left[i].abs() > 0.01 {
            worst = worst.max((right[i] / left[i] - 0.5).abs());
        }
    }
    assert!(
        worst < 0.02,
        "the inter-channel relationship was not preserved (worst deviation {worst})"
    );
}

#[test]
fn a_stereo_source_gets_one_map_that_applies_to_both_channels() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (mono, _) = make_source(
        &reference_signal,
        RATE,
        SourceSpec {
            scale: 1.0 + 20.0 / 1e6,
            offset: 25.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
        0xBEEF,
    );
    let right: Vec<f32> = mono.iter().map(|v| v * 0.5).collect();
    let source = dir.path().join("stereo.wav");
    write_wav_stereo(&source, &mono, &right, SOURCE_RATE);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    // One source, one model: the map is not per channel.
    assert_eq!(report.sources.len(), 1);
    let alignment = &report.sources[0];
    assert!(
        alignment.map.segments.len() <= 2,
        "a stereo source produced {} map segments; the map should be channel-linked",
        alignment.map.segments.len()
    );
    let model = model_for(&report, 0);
    let out = dir.path().join("corrected.wav");
    render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");

    // The two channels must have the same length and be time-aligned.
    let mut reader = WavReader::open(&out).expect("open");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read");
    assert_eq!(planes[0].len(), planes[1].len());
}

#[test]
fn material_beyond_the_end_of_the_reference_is_preserved_and_reported() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    // The source runs 30 s past the end of the reference.
    let source = build_source(
        &dir,
        "beyond.wav",
        1.0,
        25.0,
        SOURCE_RATE,
        -20.0,
        Some(DURATION - 25.0 + 30.0),
    );
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];

    let beyond = alignment
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::BeyondReference)
        .count();
    assert!(
        beyond >= 1,
        "the material past the end of the reference was not reported as such (segments: {:?})",
        alignment
            .segments
            .iter()
            .map(|s| s.kind.label())
            .collect::<Vec<_>>()
    );

    // The source-aligned render keeps the material: it must not be truncated.
    let model = model_for(&report, 0);
    let out = dir.path().join("beyond_corrected.wav");
    let result =
        render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");
    assert!(
        result.frames as f64 / result.sample_rate as f64 > DURATION - 25.0,
        "the render lost the material past the end of the reference"
    );
}

#[test]
fn a_recorder_gap_is_reported_and_never_filled_in() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let reference_signal = common::reference_signal(RATE, DURATION, 0xC0FFEE);
    let (mut samples, truth) = make_source(
        &reference_signal,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 10.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
        0xBEEF,
    );
    // The recorder stopped for 30 s in the middle of the performance.
    let gap_start = 40.0f64;
    let gap_len = 30.0f64;
    let a = (gap_start * SOURCE_RATE as f64) as usize;
    let b = ((gap_start + gap_len) * SOURCE_RATE as f64) as usize;
    for v in samples.iter_mut().take(b).skip(a) {
        *v = 0.0;
    }

    let source = dir.path().join("gap.wav");
    write_wav(&source, &samples, SOURCE_RATE, WavFormat::Float32);

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let alignment = &report.sources[0];
    common::describe(&report);

    let has_gap = alignment.segments.iter().any(|s| {
        matches!(
            s.kind,
            SegmentKind::RecorderGap | SegmentKind::LowConfidence
        )
    });
    assert!(
        has_gap,
        "the recorder gap was not reported (segments: {:?})",
        alignment
            .segments
            .iter()
            .map(|s| s.kind.label())
            .collect::<Vec<_>>()
    );

    // The material before and after the gap must still be where it belongs.
    let before = alignment
        .points
        .iter()
        .filter(|p| p.source_time < gap_start - 10.0)
        .filter(|p| p.confidence.score >= 0.5)
        .map(|p| (p.reference_time - truth.reference_time(p.source_time)) * 1000.0)
        .collect::<Vec<f64>>();
    assert!(!before.is_empty(), "no points before the gap were measured");
    let (p50, _, _) = common::stats(&before);
    assert!(
        p50 <= 10.0,
        "the material before the gap moved: p50 error {p50:.3} ms"
    );

    // The gap must not have been stretched to fill: the timeline still has a hole.
    let gap_reference_start = truth.reference_time(gap_start);
    let gap_reference_end = truth.reference_time(gap_start + gap_len);
    let covered = alignment.coverage.covered_seconds;
    assert!(
        covered < DURATION,
        "everything was reported as covered; the gap was filled in"
    );
    assert!(
        gap_reference_end - gap_reference_start > 25.0,
        "the gap should span about 30 s of reference time"
    );
}

#[test]
fn the_analysis_never_modifies_its_inputs() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = build_source(
        &dir,
        "src.wav",
        1.0 + 35.0 / 1e6,
        25.0,
        SOURCE_RATE,
        -20.0,
        None,
    );

    let before = [
        stagesync_core::audio::sha256_file(&reference).expect("hash the reference"),
        stagesync_core::audio::sha256_file(&source).expect("hash the source"),
    ];

    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);
    let out = dir.path().join("corrected.wav");
    render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");

    let after = [
        stagesync_core::audio::sha256_file(&reference).expect("hash the reference"),
        stagesync_core::audio::sha256_file(&source).expect("hash the source"),
    ];
    assert_eq!(before[0], after[0], "the reference recording was modified");
    assert_eq!(before[1], after[1], "the source recording was modified");
}

#[test]
fn the_timeline_layout_writes_silence_where_the_source_is_absent() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = build_source(&dir, "src.wav", 1.0, 25.0, SOURCE_RATE, -20.0, None);
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);

    let options = RenderOptions {
        layout: Layout::ReferenceTimelinePerSource,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("timeline.wav");
    let result =
        render_reference_timeline(&source, &model, DURATION, &out, &options).expect("render");

    assert!(
        result.silence_frames > 0,
        "the reference-length render has no silence placeholder"
    );
    let (samples, rate, _) = read_mono(&out);
    assert_eq!(rate, RATE);
    assert!(
        (samples.len() as f64 / rate as f64 - DURATION).abs() < 0.05,
        "the timeline render is {} s long, expected {DURATION} s",
        samples.len() as f64 / rate as f64
    );

    // The first 20 s of the reference are not covered by this source.
    let head = &samples[..((20.0 * rate as f64) as usize).min(samples.len())];
    let head_peak = head.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        head_peak < 1e-6,
        "the uncovered part of the reference is not silent (peak {head_peak})"
    );
}

#[test]
fn the_merged_timeline_combines_two_sources_without_losing_either() {
    let dir = TempDir::new().expect("temp dir");
    // The first source covers the beginning of the reference and the second one the
    // end, so the stretch between them is covered by neither.
    const MERGED_DURATION: f64 = 240.0;
    let reference = reference_file(RATE, MERGED_DURATION, 0xC0FFEE);
    let first = build_source_from(
        &dir,
        "first.wav",
        MERGED_DURATION,
        SourceSpec {
            scale: 1.0,
            offset: 5.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: Some(90.0),
        },
    );
    let second = build_source_from(
        &dir,
        "second.wav",
        MERGED_DURATION,
        SourceSpec {
            scale: 1.0,
            offset: 140.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs: None,
        },
    );

    let report = analyse(&reference, &[first.clone(), second.clone()], &config());
    let model_first = model_for(&report, 0);
    let model_second = model_for(&report, 1);

    let options = RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(
        &[(first.clone(), model_first), (second.clone(), model_second)],
        MERGED_DURATION,
        &out,
        &options,
    )
    .expect("render");

    assert!(result.frames > 0, "the merged render is empty");
    let (samples, _, _) = read_mono(&out);
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.05, "the merged render is essentially silent");

    // Somewhere in the middle there must be a hole: neither source covers it. The
    // first source covers the beginning of the reference and the second one the end,
    // so the hole is the stretch between them.
    let hole_start = model_first.reference_time(90.0);
    let hole_end = model_second.reference_time(0.0);
    assert!(
        hole_end - hole_start > 5.0,
        "the fixture does not leave a hole: {hole_start}..{hole_end}"
    );
    let a = (hole_start * RATE as f64) as usize;
    let b = (hole_end * RATE as f64) as usize;
    let region = &samples[a.min(samples.len())..b.min(samples.len())];
    let region_peak = region.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let mut sq = 0.0f64;
    for v in region {
        sq += (*v as f64).powi(2);
    }
    let region_rms = (sq / region.len().max(1) as f64).sqrt();

    // The hole must not be filled with material. It is not *exactly* zero: the
    // windowed-sinc resampler rings for a few milliseconds after the last sample it
    // could use, and that ringing belongs to the source, so it is left in place. It
    // has to sit far below the signal, though.
    let signal_peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        region_peak < signal_peak * 0.01,
        "the gap between the two sources was filled with material (peak {region_peak}, signal {signal_peak})"
    );
    assert!(
        region_rms < 1e-4,
        "the gap between the two sources is not silent (rms {region_rms})"
    );
}

#[test]
fn the_pitch_change_follows_the_drift() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let scale = 1.0 + 1000.0 / 1e6;
    let source = build_source(&dir, "src.wav", scale, 25.0, SOURCE_RATE, -20.0, None);
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);

    // A clock that runs 1000 ppm slow stretches the recording, so the correction
    // resamples it and the result is a little lower in pitch:
    // -1200 * log2(scale) cents.
    let truth_cents = -1200.0 * scale.log2();
    assert!(
        (truth_cents + 1.7306).abs() < 1e-3,
        "the arithmetic for 1000 ppm is wrong: {truth_cents} cents"
    );

    // The model must have measured a rate close enough to the truth for the correction
    // to be meaningful at all.
    assert!(
        (model.scale - scale).abs() < 200e-6,
        "the measured rate {} is too far from the truth {scale}",
        model.scale
    );

    let expected = -1200.0 * model.scale.log2();
    assert!(
        expected < 0.0,
        "a source whose clock ran slow must be corrected downwards in pitch, got {expected}"
    );

    let out = dir.path().join("corrected.wav");
    let result =
        render_source_aligned(&source, &model, &out, &RenderOptions::default()).expect("render");
    assert!(
        (result.pitch_shift_cents - expected).abs() < 1e-9,
        "the renderer reported {} cents, the model implies {expected}",
        result.pitch_shift_cents
    );
    assert!(
        result.pitch_shift_cents < 0.0,
        "a source whose clock ran slow must be pitched down"
    );
}

#[test]
fn an_int16_render_preserves_the_level_without_clipping() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let source = build_source(&dir, "src.wav", 1.0, 25.0, SOURCE_RATE, -20.0, None);
    let report = analyse(&reference, std::slice::from_ref(&source), &config());
    let model = model_for(&report, 0);

    let options = RenderOptions {
        format: WavFormat::Int16,
        ..RenderOptions::default()
    };
    let out = dir.path().join("corrected_i16.wav");
    render_source_aligned(&source, &model, &out, &options).expect("render");

    let mut reader = WavReader::open(&out).expect("open");
    assert_eq!(reader.sample_rate(), SOURCE_RATE);
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read");
    let peak = planes[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 1.0, "the int16 render clips at {peak}");
    assert!(peak > 0.05, "the int16 render is essentially silent");
}

#[test]
fn a_writer_can_produce_a_file_the_reader_can_read_back() {
    // A tiny end-to-end check of the WAV layer the exports depend on.
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("roundtrip.wav");
    let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 100.0).sin() * 0.5).collect();
    {
        let mut writer = WavWriter::create(&path, 44_100, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");
    }
    let mut reader = WavReader::open(&path).expect("open");
    assert_eq!(reader.sample_rate(), 44_100);
    assert_eq!(reader.channels(), 1);
    assert_eq!(reader.frames(), 1000);
    let planes = reader.read_frames(0, 1000).expect("read");
    for (i, v) in planes[0].iter().enumerate() {
        assert!(
            (v - samples[i]).abs() < 1e-6,
            "sample {i} did not survive the round trip"
        );
    }
}
