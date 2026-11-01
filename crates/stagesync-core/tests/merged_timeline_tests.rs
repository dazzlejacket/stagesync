//! Regression tests for the merged timeline's cursor accounting.
//!
//! The merged timeline is written by a cursor that walks each source's material
//! once and hands it to the renderer span by span. The cursor's position on the
//! timeline has to advance by exactly the frames that leave it — the frames
//! discarded because they lie before the requested span, and the frames handed
//! over. When it did not, the next span computed its skip from a stale origin,
//! threw away material it had already given away and read from further along
//! the recording than the plan asked for.
//!
//! The symptom was severe and completely silent: on the real recordings the
//! merged export kept about half the material it was supposed to contain, and
//! what it kept was displaced by 15 to 29 seconds.
//!
//! These tests check the **rendered audio**, not the renderer's own report: the
//! output is decoded independently and compared frame by frame against the
//! timeline the plan describes. A test that only measured the file's duration,
//! or that trusted `RenderResult::source_frames`, would have passed while the
//! audio was wrong.

mod common;

use std::path::Path;

use common::write_wav;
use stagesync_core::audio::{AudioDecoder, WavFormat, WavReader, WavWriter};
use stagesync_core::matching::fine::SkipReasons;
use stagesync_core::matching::segments::{Coverage, TimelineSpan};
use stagesync_core::matching::{
    AlignmentReport, Confidence, LinearModel, SegmentKind, SourceAlignment, TimeMap,
    UncertaintyReason,
};
use stagesync_core::render::{render_merged_timeline, Layout, RenderOptions};
use tempfile::TempDir;

const RATE: u32 = 8000;

/// A deterministic signal whose value identifies the sample's own index.
///
/// The value is never zero, so "this frame carries material" and "this frame is
/// silence" cannot be confused, and the period is short enough that a frame
/// read from the wrong place is visibly wrong.
fn ramp(n: usize, period: usize, amplitude: f32, phase: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amplitude * (phase + ((i % period) as f32 / period as f32)))
        .collect()
}

/// A source alignment carrying the model the test wants applied.
///
/// The matcher is not needed to pin down a rendering defect: what matters is the
/// model, the plan and the file. Building the report by hand keeps the test from
/// depending on the matcher finding anything.
fn alignment(path: &Path, model: LinearModel, duration: f64) -> SourceAlignment {
    let decoder = AudioDecoder::open(path).expect("open the source");
    let meta = decoder.meta().clone();
    SourceAlignment {
        source: meta,
        map: TimeMap::from_linear(model, duration),
        coarse: None,
        fine_measured: 0,
        fine_skipped: 0,
        fine_skip_reasons: SkipReasons::default(),
        direct_model: Some(model),
        calibrated_model: None,
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

/// A report with the given sources and the given ownership plan.
fn report(
    reference: &Path,
    sources: Vec<SourceAlignment>,
    timeline: Vec<TimelineSpan>,
    timeline_duration: f64,
) -> AlignmentReport {
    let decoder = AudioDecoder::open(reference).expect("open the reference");
    let meta = decoder.meta().clone();
    AlignmentReport {
        reference: meta,
        sources,
        timeline,
        timeline_duration,
    }
}

/// A span of the timeline a source owns.
fn owned(start: f64, end: f64, source_index: usize) -> TimelineSpan {
    TimelineSpan {
        reference_start: start,
        reference_end: end,
        kind: SegmentKind::Matched,
        source_index: Some(source_index),
        confidence: Confidence::new(1.0, UncertaintyReason::LowEnergy),
        note: String::new(),
    }
}

/// A span of the timeline no source covers.
fn uncovered(start: f64, end: f64) -> TimelineSpan {
    TimelineSpan {
        reference_start: start,
        reference_end: end,
        kind: SegmentKind::UncoveredReference,
        source_index: None,
        confidence: Confidence::new(0.0, UncertaintyReason::LowEnergy),
        note: "no recorder covered this span of the reference".into(),
    }
}

/// Write a stereo WAV file.
fn write_wav_stereo_file(path: &Path, left: &[f32], right: &[f32], rate: u32) {
    let frames = left.len().min(right.len());
    let mut writer = WavWriter::create(path, rate, 2, WavFormat::Float32).expect("create");
    writer
        .write_planar(&[left[..frames].to_vec(), right[..frames].to_vec()])
        .expect("write");
    writer.finalize().expect("finalise");
}

/// The merged renderer's options.
fn merged_options() -> RenderOptions {
    RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    }
}

/// Compare two signals frame by frame, naming the first frame that differs.
fn assert_same_signal(label: &str, got: &[f32], want: &[f32]) {
    assert_eq!(
        got.len(),
        want.len(),
        "{label}: the rendered signal is {} frames long, not {}",
        got.len(),
        want.len()
    );
    for (index, (a, b)) in got.iter().zip(want.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "{label}: frame {index} is {a}, not {b}",
        );
    }
}

// ---------------------------------------------------------------------------
// The merged timeline places every span where the plan says
// ---------------------------------------------------------------------------

/// Every span must land at the reference position the plan claims, and the
/// amount of source-backed audio must be the amount the plan promises.
///
/// The plan hands the first source two spans that are twenty seconds apart and
/// gives the stretch between them to a second source. The first source's own
/// file carries twenty seconds of silence there — that is the recorder's gap,
/// and the plan says so — so the cursor has to skip over its own silence and
/// pick the recording up again exactly where the plan says.
///
/// The cursor's position used to advance only for the frames it *skipped*, never
/// for the frames it handed over. The second span therefore computed its skip
/// from the origin the first span started at, discarded twenty seconds of
/// material it had already given away, and read the wrong twenty seconds of the
/// recording. The output was short and displaced, and nothing said so.
#[test]
fn a_merged_timeline_places_every_span_where_the_plan_says() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 90.0;
    let total = (DURATION * RATE as f64) as usize;
    let twenty = 20 * RATE as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    // The first source: the first twenty seconds, twenty seconds of its own
    // recorder gap, and then the rest of the performance.
    let continuous = ramp(total, 4096, 0.5, 0.1);
    let mut first_samples = Vec::with_capacity(total);
    first_samples.extend_from_slice(&continuous[..twenty]);
    first_samples.extend(std::iter::repeat_n(0.0f32, twenty));
    first_samples.extend_from_slice(&continuous[2 * twenty..]);
    let first = dir.path().join("first.wav");
    write_wav(&first, &first_samples, RATE, WavFormat::Float32);

    // The second source: the twenty seconds in between, placed twenty seconds
    // into the reference.
    let second_samples = ramp(twenty, 2048, 0.4, 0.2);
    let second = dir.path().join("second.wav");
    write_wav(&second, &second_samples, RATE, WavFormat::Float32);

    let report = report(
        &reference,
        vec![
            alignment(
                &first,
                LinearModel {
                    scale: 1.0,
                    offset: 0.0,
                },
                DURATION,
            ),
            alignment(
                &second,
                LinearModel {
                    scale: 1.0,
                    offset: 20.0,
                },
                20.0,
            ),
        ],
        vec![
            owned(0.0, 20.0, 0),
            owned(20.0, 40.0, 1),
            owned(40.0, 90.0, 0),
        ],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &merged_options()).expect("render");

    // The file has exactly the reference's length.
    assert_eq!(
        result.frames, total as u64,
        "the merged timeline is {} frames long, not {}",
        result.frames, total
    );

    // The expected timeline, written out by hand from the plan.
    let mut expected = vec![0.0f32; total];
    expected[..twenty].copy_from_slice(&first_samples[..twenty]);
    expected[twenty..2 * twenty].copy_from_slice(&second_samples[..]);
    expected[2 * twenty..].copy_from_slice(&first_samples[2 * twenty..]);

    // The rendered audio must be that timeline, frame for frame.
    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    assert_same_signal("the merged timeline", &planes[0], &expected);

    // The amount of source-backed audio must equal the amount the plan claims.
    assert_eq!(
        result.source_frames, total as u64,
        "the renderer wrote {} frames of source audio, not {}",
        result.source_frames, total
    );
    assert_eq!(
        result.silence_frames, 0,
        "the renderer wrote {} frames of silence, but the plan covers the whole timeline",
        result.silence_frames
    );
}

// ---------------------------------------------------------------------------
// The cursor survives several spans, a source transition and a return
// ---------------------------------------------------------------------------

/// A source that owns several spans must be read correctly at each of them.
///
/// The cursor is created when a source's first span is reached and is kept for
/// the rest of the render, so a source that comes back later is read from the
/// middle of its own material. The spans here alternate between two sources,
/// so the render crosses a source transition, comes back to the first source,
/// and crosses back again.
#[test]
fn a_merged_timeline_reads_a_source_again_at_a_later_span() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 120.0;
    let total = (DURATION * RATE as f64) as usize;
    let span = 30 * RATE as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    // The first source owns four of the five spans. Its file carries the whole
    // performance: the spans it does not own are covered by the other source,
    // not by its own silence.
    let first_samples = ramp(total, 4096, 0.5, 0.1);
    let first = dir.path().join("first.wav");
    write_wav(&first, &first_samples, RATE, WavFormat::Float32);

    // The second source owns the middle span and is placed at the start of it.
    let second_samples = ramp(span, 2048, 0.4, 0.2);
    let second = dir.path().join("second.wav");
    write_wav(&second, &second_samples, RATE, WavFormat::Float32);

    let report = report(
        &reference,
        vec![
            alignment(
                &first,
                LinearModel {
                    scale: 1.0,
                    offset: 0.0,
                },
                DURATION,
            ),
            alignment(
                &second,
                LinearModel {
                    scale: 1.0,
                    offset: 60.0,
                },
                30.0,
            ),
        ],
        vec![
            owned(0.0, 30.0, 0),
            owned(30.0, 60.0, 0),
            owned(60.0, 90.0, 1),
            owned(90.0, 120.0, 0),
        ],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &merged_options()).expect("render");
    assert_eq!(result.frames, total as u64);

    let mut expected = vec![0.0f32; total];
    expected[..2 * span].copy_from_slice(&first_samples[..2 * span]);
    expected[2 * span..3 * span].copy_from_slice(&second_samples[..]);
    expected[3 * span..].copy_from_slice(&first_samples[3 * span..]);

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    assert_same_signal("the merged timeline", &planes[0], &expected);
    assert_eq!(result.source_frames, total as u64);
    assert_eq!(result.silence_frames, 0);
}

// ---------------------------------------------------------------------------
// A source that runs out before its span does
// ---------------------------------------------------------------------------

/// A source that ends early leaves the rest of its span as silence, and the
/// renderer says how much material it actually wrote.
///
/// The cursor must advance by what it really returned, not by what it was asked
/// for. A short read that advanced the cursor by the requested length would
/// leave it pointing past material it still holds.
#[test]
fn a_merged_timeline_leaves_a_short_source_silent_and_says_so() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 60.0;
    let total = (DURATION * RATE as f64) as usize;
    let half = 30 * RATE as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    // The source is only half as long as the span the plan gives it.
    let first_samples = ramp(half, 4096, 0.5, 0.1);
    let first = dir.path().join("first.wav");
    write_wav(&first, &first_samples, RATE, WavFormat::Float32);

    let report = report(
        &reference,
        vec![alignment(
            &first,
            LinearModel {
                scale: 1.0,
                offset: 0.0,
            },
            60.0,
        )],
        vec![owned(0.0, 60.0, 0)],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &merged_options()).expect("render");

    assert_eq!(result.frames, total as u64, "the timeline lost its length");

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");

    let mut expected = vec![0.0f32; total];
    expected[..half].copy_from_slice(&first_samples[..]);
    assert_same_signal("the merged timeline", &planes[0], &expected);

    // The renderer must report the material it really wrote, and the silence it
    // really left.
    assert_eq!(result.source_frames, half as u64);
    assert_eq!(result.silence_frames, (total - half) as u64);
}

// ---------------------------------------------------------------------------
// A gap in the plan is silence, and the channels are carried
// ---------------------------------------------------------------------------

/// A span no source covers is silence, and a stereo source keeps its two
/// channels in every span it owns.
///
/// The two checks share a render because they share a failure mode: a cursor
/// that loses its place does not only move the material, it moves the silence
/// with it.
#[test]
fn a_merged_timeline_keeps_its_channels_and_its_gap() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 60.0;
    let total = (DURATION * RATE as f64) as usize;
    let thirty = 30 * RATE as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    // A stereo source whose two channels are different signals.
    let left = ramp(total, 4096, 0.5, 0.1);
    let right = ramp(total, 2048, 0.3, 0.4);
    let first = dir.path().join("first.wav");
    write_wav_stereo_file(&first, &left, &right, RATE);

    let report = report(
        &reference,
        vec![alignment(
            &first,
            LinearModel {
                scale: 1.0,
                offset: 0.0,
            },
            DURATION,
        )],
        vec![owned(0.0, 30.0, 0), uncovered(30.0, 60.0)],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &merged_options()).expect("render");

    assert_eq!(result.channels, 2, "the merged timeline lost a channel");
    assert_eq!(result.frames, total as u64);

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");

    // The first span carries the source's own two channels.
    assert_same_signal("the left channel", &planes[0][..thirty], &left[..thirty]);
    assert_same_signal("the right channel", &planes[1][..thirty], &right[..thirty]);

    // The span no source covers is silence, in every channel.
    for channel in &planes {
        for (index, value) in channel[thirty..].iter().enumerate() {
            assert_eq!(
                *value,
                0.0,
                "frame {} of an uncovered span is {value}, not silence",
                thirty + index
            );
        }
    }

    assert_eq!(result.source_frames, thirty as u64);
    assert_eq!(result.silence_frames, (total - thirty) as u64);
}

// ---------------------------------------------------------------------------
// The exact output boundaries
// ---------------------------------------------------------------------------

/// The merged timeline ends exactly at the reference's last frame, and the last
/// span's material is there.
///
/// A span that ends one frame early, or a timeline that is one frame short,
/// loses audio that the plan promised.
#[test]
fn a_merged_timeline_ends_exactly_at_the_reference() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 45.0;
    let total = (DURATION * RATE as f64) as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    let first_samples = ramp(total, 4096, 0.5, 0.1);
    let first = dir.path().join("first.wav");
    write_wav(&first, &first_samples, RATE, WavFormat::Float32);

    let report = report(
        &reference,
        vec![alignment(
            &first,
            LinearModel {
                scale: 1.0,
                offset: 0.0,
            },
            DURATION,
        )],
        vec![owned(0.0, DURATION, 0)],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &merged_options()).expect("render");

    assert_eq!(
        result.frames, total as u64,
        "the merged timeline is {} frames long, not {}",
        result.frames, total
    );

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    assert_eq!(
        frames, total,
        "the written file is not the reference's length"
    );
    let planes = reader.read_frames(0, frames).expect("read the render");
    assert_same_signal("the merged timeline", &planes[0], &first_samples);
}

// ---------------------------------------------------------------------------
// The merged renderer does not claim to have resampled when it did not
// ---------------------------------------------------------------------------

/// The merged renderer must derive its `resampled` flag from the geometry it
/// used, like the two per-source layouts do.
///
/// The flag used to be hardcoded to `true`, so a merged timeline whose sources
/// all needed no rate change claimed that it had been through a filter.
#[test]
fn a_merged_timeline_reports_that_it_did_not_resample() {
    let dir = TempDir::new().expect("temp dir");

    const DURATION: f64 = 10.0;
    let total = (DURATION * RATE as f64) as usize;

    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &ramp(total, 4096, 0.5, 0.1),
        RATE,
        WavFormat::Float32,
    );

    let first_samples = ramp(total, 4096, 0.5, 0.1);
    let first = dir.path().join("first.wav");
    write_wav(&first, &first_samples, RATE, WavFormat::Float32);

    let unity = report(
        &reference,
        vec![alignment(
            &first,
            LinearModel {
                scale: 1.0,
                offset: 0.0,
            },
            DURATION,
        )],
        vec![owned(0.0, DURATION, 0)],
        DURATION,
    );

    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&unity, &out, &merged_options()).expect("render");
    assert!(
        !result.resampled,
        "the merged timeline claimed to have resampled material it copied"
    );

    // A source that does need a rate change must still be reported as resampled.
    let drifting = report(
        &reference,
        vec![alignment(
            &first,
            LinearModel {
                scale: 1.000_016,
                offset: 0.0,
            },
            DURATION,
        )],
        vec![owned(0.0, DURATION, 0)],
        DURATION,
    );
    let out = dir.path().join("merged-resampled.wav");
    let result = render_merged_timeline(&drifting, &out, &merged_options()).expect("render");
    assert!(
        result.resampled,
        "the merged timeline did not report the resampling it did"
    );
}
