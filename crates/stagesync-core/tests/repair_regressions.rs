//! Regression tests for the defects the post-release audit found.
//!
//! Every test names the defect it pins down. A test that fails says which
//! behaviour regressed, rather than leaving it to be diagnosed from a symptom.
//!
//! The defects are grouped by the promise they broke:
//!
//! * the resampler's position map (P1-03),
//! * the merged timeline's channels and rates (P1-02),
//! * the coverage plan (P1-04),
//! * exact output lengths (P1-05) and the unity bypass (P2-03),
//! * the writer's temporary files (P1-06) and its size limit (P1-07),
//! * the reader's frame count and scale (P2-02).

mod common;

use std::path::Path;

use common::{reference_file, reference_signal, write_wav};
use stagesync_core::audio::{
    ensure_wav_size_fits, AudioDecoder, WavFormat, WavReader, WavWriter, RIFF_MAX_DATA_BYTES,
};
use stagesync_core::matching::fine::SkipReasons;
use stagesync_core::matching::segments::{Coverage, TimelineSpan};
use stagesync_core::matching::{
    AlignmentReport, Confidence, LinearModel, Segment, SegmentKind, SourceAlignment, TimeMap,
    UncertaintyReason,
};
use stagesync_core::render::{
    render_merged_timeline, render_reference_timeline, render_source_aligned, Layout, RenderOptions,
};
use stagesync_core::resample::StreamingResampler;
use tempfile::TempDir;

const RATE: u32 = 8000;
const OTHER_RATE: u32 = 16000;
const DURATION: f64 = 30.0;

/// A source alignment carrying the model the test wants applied.
///
/// The analysis is not needed to pin down a rendering defect: what matters is
/// the model, the plan and the file. Building the report by hand keeps the
/// test from depending on the matcher finding anything.
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

/// The source's material is placed on the reference timeline by the model.
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

/// A sine wave.
fn sine(rate: u32, seconds: f64, frequency: f64, amplitude: f32) -> Vec<f32> {
    let n = (rate as f64 * seconds).round() as usize;
    (0..n)
        .map(|i| {
            amplitude
                * (2.0 * std::f64::consts::PI * frequency * i as f64 / rate as f64).sin() as f32
        })
        .collect()
}

/// The magnitude of `frequency` in `samples`, by the Goertzel method.
fn goertzel(samples: &[f32], rate: u32, frequency: f64) -> f64 {
    let n = samples.len() as f64;
    let k = 2.0 * std::f64::consts::PI * frequency / rate as f64;
    let coeff = 2.0 * k.cos();
    let mut previous = 0.0f64;
    let mut before_previous = 0.0f64;
    for sample in samples {
        let current = *sample as f64 + coeff * previous - before_previous;
        before_previous = previous;
        previous = current;
    }
    (previous * previous + before_previous * before_previous - coeff * previous * before_previous)
        .sqrt()
        / n
}

// ---------------------------------------------------------------------------
// P1-03 — the resampler's position map
// ---------------------------------------------------------------------------

/// The resampler must not lose the input that lies before the first output it
/// is asked for.
///
/// The origin of the buffer used to be advanced by the number of samples the
/// *next* output would need, even when fewer samples than that were actually
/// discarded. Once the origin ran ahead of the buffer, every later sample was
/// read from the wrong place, and the ones before the new origin were read as
/// silence.
#[test]
fn the_resampler_keeps_the_samples_before_a_late_start() {
    let input: Vec<f32> = (0..4000).map(|i| (i as f32) / 4000.0).collect();

    // The same map, produced in one block and in small blocks. The map is
    // absolute, so the two must agree exactly.
    let mut whole = StreamingResampler::new(1.0, 0.0, 63, 1024);
    whole.set_next_output(1000);
    let mut chunked = StreamingResampler::new(1.0, 0.0, 63, 1024);
    chunked.set_next_output(1000);

    let mut whole_out = whole.push(&input);
    whole_out.extend(whole.flush());

    let mut chunked_out = Vec::new();
    for block in input.chunks(100) {
        chunked_out.extend(chunked.push(block));
    }
    chunked_out.extend(chunked.flush());

    assert_eq!(
        whole_out.len(),
        chunked_out.len(),
        "the two chunkings produced different lengths"
    );
    for (index, (a, b)) in whole_out.iter().zip(chunked_out.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "output {index} differs: {a} against {b}"
        );
    }

    // And the values must be the input's, not silence: the output frame 1000
    // is taken from input frame 1000.
    let start = whole_out
        .iter()
        .position(|v| v.abs() > 1e-6)
        .expect("the first output should not be silence");
    assert_eq!(start, 0, "the first output frame was silence");
    assert!(
        (whole_out[0] - input[1000]).abs() < 0.02,
        "the first output frame is {}, not the input's {}",
        whole_out[0],
        input[1000]
    );
}

// ---------------------------------------------------------------------------
// P1-02 — the merged timeline's channels and rates
// ---------------------------------------------------------------------------

/// The merged timeline must carry every channel of a source.
///
/// The channels used to be averaged into one before the position map was
/// applied. A stereo source whose two channels differ therefore came out with
/// the same signal in both, and a source whose channels were in antiphase came
/// out silent.
#[test]
fn a_merged_timeline_keeps_both_channels_of_a_stereo_source() {
    let dir = TempDir::new().expect("temp dir");
    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &sine(RATE, DURATION, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );

    // A stereo source whose right channel is a quarter of the left.
    let left = sine(RATE, DURATION, 440.0, 0.8);
    let right: Vec<f32> = left.iter().map(|v| v * 0.25).collect();
    let source = dir.path().join("stereo.wav");
    write_wav_stereo_file(&source, &left, &right, RATE);

    let model = LinearModel {
        scale: 1.0,
        offset: 0.0,
    };
    let report = report(
        &reference,
        vec![alignment(&source, model, DURATION)],
        vec![owned(0.0, DURATION, 0)],
        DURATION,
    );

    let options = RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("merged.wav");
    let result = render_merged_timeline(&report, &out, &options).expect("render");
    assert_eq!(result.channels, 2, "the merged timeline lost a channel");

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    let left = &planes[0];
    let right = &planes[1];

    // The ratio between the channels must survive the render. Averaging the
    // channels into one and writing that into both would give a ratio of one.
    let mut checked = 0;
    for index in 0..frames {
        if left[index].abs() < 0.05 {
            continue;
        }
        let ratio = right[index] / left[index];
        assert!(
            (ratio - 0.25).abs() < 0.02,
            "frame {index}: the right channel is {ratio} of the left, not 0.25"
        );
        checked += 1;
    }
    assert!(checked > 100, "not enough frames were loud enough to check");
}

/// The merged timeline must read each source at its own rate.
///
/// The rate of the *first* source used to be applied to every source. A 16 kHz
/// source placed in an 8 kHz timeline was therefore read as if it were 8 kHz
/// material, which plays its content at half speed.
#[test]
fn a_merged_timeline_reads_each_source_at_its_own_rate() {
    let dir = TempDir::new().expect("temp dir");
    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &sine(RATE, DURATION, 220.0, 0.5),
        RATE,
        WavFormat::Float32,
    );

    // The first source is 8 kHz, so the timeline's rate is 8 kHz.
    let first = dir.path().join("first.wav");
    write_wav(
        &first,
        &sine(RATE, DURATION, 220.0, 0.4),
        RATE,
        WavFormat::Float32,
    );

    // The second source is 16 kHz and carries a 1 kHz tone. Read at its own
    // rate and decimated to 8 kHz, the tone is still a 1 kHz tone. Read at the
    // first source's rate, it comes out at 500 Hz.
    let second = dir.path().join("second.wav");
    write_wav(
        &second,
        &sine(OTHER_RATE, DURATION, 1000.0, 0.5),
        OTHER_RATE,
        WavFormat::Float32,
    );

    let model = LinearModel {
        scale: 1.0,
        offset: 0.0,
    };
    let report = report(
        &reference,
        vec![
            alignment(&first, model, DURATION),
            alignment(&second, model, DURATION),
        ],
        vec![owned(0.0, DURATION, 1)],
        DURATION,
    );

    let options = RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("merged.wav");
    render_merged_timeline(&report, &out, &options).expect("render");

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    let mono = &planes[0];

    let wanted = goertzel(mono, RATE, 1000.0);
    let wrong = goertzel(mono, RATE, 500.0);
    assert!(
        wanted > wrong * 4.0,
        "the 1 kHz tone came out at the wrong rate: 1 kHz magnitude {wanted}, 500 Hz magnitude {wrong}"
    );
}

/// A merged timeline must refuse an output narrower than one of its sources.
///
/// Writing a narrower output would drop a channel of real audio.
#[test]
fn a_merged_timeline_refuses_to_drop_a_channel() {
    let dir = TempDir::new().expect("temp dir");
    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &sine(RATE, 2.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );
    let left = sine(RATE, 2.0, 440.0, 0.8);
    let right: Vec<f32> = left.iter().map(|v| v * 0.25).collect();
    let source = dir.path().join("stereo.wav");
    write_wav_stereo_file(&source, &left, &right, RATE);

    let model = LinearModel {
        scale: 1.0,
        offset: 0.0,
    };
    let report = report(
        &reference,
        vec![alignment(&source, model, 2.0)],
        vec![owned(0.0, 2.0, 0)],
        2.0,
    );

    let options = RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        channels: Some(1),
        ..RenderOptions::default()
    };
    let out = dir.path().join("merged.wav");
    let error = render_merged_timeline(&report, &out, &options)
        .expect_err("a one-channel merged timeline of a stereo source must be refused");
    let message = format!("{error:#}");
    assert!(
        message.contains("channel"),
        "the refusal does not say what is wrong: {message}"
    );
}

// ---------------------------------------------------------------------------
// P1-04 — the coverage plan
// ---------------------------------------------------------------------------

/// A span no source covers must be silence in the merged render, and the
/// material either side of it must be there.
///
/// The plan is the authority: the render may not fill a hole, and it may not
/// leave a hole where the plan says there is material.
#[test]
fn a_span_no_source_covers_is_silence_in_the_merged_render() {
    let dir = TempDir::new().expect("temp dir");
    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &sine(RATE, 20.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );
    let source = dir.path().join("source.wav");
    write_wav(
        &source,
        &sine(RATE, 20.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );

    let model = LinearModel {
        scale: 1.0,
        offset: 0.0,
    };
    let report = report(
        &reference,
        vec![alignment(&source, model, 20.0)],
        vec![
            owned(0.0, 5.0, 0),
            uncovered(5.0, 10.0),
            owned(10.0, 20.0, 0),
        ],
        20.0,
    );

    let options = RenderOptions {
        layout: Layout::ReferenceTimelineMerged,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("merged.wav");
    render_merged_timeline(&report, &out, &options).expect("render");

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    let mono = &planes[0];

    let peak = |from: usize, to: usize| -> f32 {
        mono[from..to].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    assert!(
        peak(5 * RATE as usize, 10 * RATE as usize) < 1e-6,
        "the uncovered span was filled with material"
    );
    assert!(
        peak(0, 5 * RATE as usize) > 0.1,
        "the covered span before the hole is missing"
    );
    assert!(
        peak(10 * RATE as usize, 20 * RATE as usize) > 0.1,
        "the covered span after the hole is missing"
    );
}

/// A source that covers a span inside another source's span must not cut a
/// hole in the timeline.
///
/// The merge used to walk the intervals in order and move a cursor to each
/// interval's end. A nested interval moved that cursor backwards, and the
/// interval after it was reported as a gap even though the first source covered
/// it throughout.
#[test]
fn a_nested_source_does_not_cut_a_hole_in_the_timeline() {
    use stagesync_core::matching::segments::merge_timeline;

    let outer = Segment {
        kind: SegmentKind::Matched,
        source_index: Some(0),
        source_start: Some(0.0),
        source_end: Some(10.0),
        reference_start: 0.0,
        reference_end: 10.0,
        confidence: Confidence::new(1.0, UncertaintyReason::LowEnergy),
        note: String::new(),
    };
    let inner = Segment {
        kind: SegmentKind::Matched,
        source_index: Some(1),
        source_start: Some(0.0),
        source_end: Some(1.0),
        reference_start: 2.0,
        reference_end: 3.0,
        confidence: Confidence::new(1.0, UncertaintyReason::LowEnergy),
        note: String::new(),
    };

    let spans = merge_timeline(&[vec![outer], vec![inner]], 10.0);

    // Nothing may be reported as uncovered: the first source covers the whole
    // reference.
    assert!(
        !spans
            .iter()
            .any(|s| s.kind == SegmentKind::UncoveredReference),
        "a hole was invented inside a fully covered range: {spans:?}"
    );

    // And the spans must cover the whole reference with no gap between them.
    assert_eq!(spans.first().map(|s| s.reference_start), Some(0.0));
    assert_eq!(spans.last().map(|s| s.reference_end), Some(10.0));
    for pair in spans.windows(2) {
        assert!(
            (pair[0].reference_end - pair[1].reference_start).abs() < 1e-6,
            "the spans do not join: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

/// Where two sources overlap, the higher-priority one owns the overlap.
///
/// The lower-priority source still owns what the higher-priority one does not
/// reach, so the timeline keeps both contributions rather than dropping the
/// second one entirely.
#[test]
fn overlapping_sources_are_resolved_by_priority() {
    use stagesync_core::matching::segments::merge_timeline;

    let segment = |source_index: usize, start: f64, end: f64| Segment {
        kind: SegmentKind::Matched,
        source_index: Some(source_index),
        source_start: Some(start),
        source_end: Some(end),
        reference_start: start,
        reference_end: end,
        confidence: Confidence::new(1.0, UncertaintyReason::LowEnergy),
        note: String::new(),
    };

    // The first source covers the whole reference, so it owns all of it.
    let spans = merge_timeline(
        &[vec![segment(0, 0.0, 10.0)], vec![segment(1, 4.0, 10.0)]],
        10.0,
    );
    assert_eq!(
        spans.len(),
        1,
        "the timeline should have one span: {spans:?}"
    );
    assert_eq!(spans[0].source_index, Some(0));
    assert_eq!(spans[0].reference_start, 0.0);
    assert_eq!(spans[0].reference_end, 10.0);

    // The first source stops halfway, so the second one takes over from there.
    let spans = merge_timeline(
        &[vec![segment(0, 0.0, 4.0)], vec![segment(1, 4.0, 10.0)]],
        10.0,
    );
    assert_eq!(
        spans.len(),
        2,
        "the timeline should have two spans: {spans:?}"
    );
    assert_eq!(spans[0].source_index, Some(0));
    assert_eq!(spans[0].reference_start, 0.0);
    assert_eq!(spans[0].reference_end, 4.0);
    assert_eq!(spans[1].source_index, Some(1));
    assert_eq!(spans[1].reference_start, 4.0);
    assert_eq!(spans[1].reference_end, 10.0);
}

// ---------------------------------------------------------------------------
// P1-05 — exact output lengths
// ---------------------------------------------------------------------------

/// A reference-timeline render must have exactly the reference's duration.
///
/// It used to be a little longer, because the resampler's tail was written
/// past the end of the content.
#[test]
fn a_reference_timeline_has_exactly_the_reference_duration() {
    let dir = TempDir::new().expect("temp dir");
    let source = dir.path().join("source.wav");
    write_wav(
        &source,
        &sine(RATE, 10.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );

    let model = LinearModel {
        scale: 1.0 + 35.0 / 1e6,
        offset: 2.0,
    };
    let options = RenderOptions {
        layout: Layout::ReferenceTimelinePerSource,
        sample_rate: Some(RATE),
        ..RenderOptions::default()
    };
    let out = dir.path().join("timeline.wav");
    let result = render_reference_timeline(&source, &model, 20.0, &out, &options).expect("render");

    let expected = (20.0 * RATE as f64).round() as u64;
    assert_eq!(
        result.frames, expected,
        "the timeline is {} frames, not {expected}",
        result.frames
    );
    let reader = WavReader::open(&out).expect("open the render");
    assert_eq!(reader.frames(), expected, "the file holds the wrong frames");
}

/// A source-aligned render must have exactly the length the rate change
/// implies, and not one frame more.
#[test]
fn a_source_aligned_render_has_exactly_the_implied_length() {
    let dir = TempDir::new().expect("temp dir");
    let source = dir.path().join("source.wav");
    let samples = sine(RATE, 10.0, 440.0, 0.5);
    write_wav(&source, &samples, RATE, WavFormat::Float32);

    let model = LinearModel {
        scale: 1.0 + 1000.0 / 1e6,
        offset: 0.0,
    };
    let options = RenderOptions {
        layout: Layout::SourceAligned,
        ..RenderOptions::default()
    };
    let out = dir.path().join("corrected.wav");
    let result = render_source_aligned(&source, &model, &out, &options).expect("render");

    let expected = (samples.len() as f64 * model.scale).round() as u64;
    assert_eq!(
        result.frames, expected,
        "the render is {} frames, not {expected}",
        result.frames
    );
}

// ---------------------------------------------------------------------------
// P2-03 — the unity bypass
// ---------------------------------------------------------------------------

/// A correction that needs no rate change must copy the samples.
///
/// The promise is that an offset-only correction performs no resampling. A
/// resampled file has been through a filter, so the copy is not a detail: it is
/// the difference between the material as it was recorded and the material as
/// a filter left it.
#[test]
fn a_unity_correction_copies_the_samples_instead_of_filtering_them() {
    let dir = TempDir::new().expect("temp dir");
    let source = dir.path().join("source.wav");
    // A signal with structure a filter would change: a sharp transient.
    let mut samples = sine(RATE, 2.0, 440.0, 0.5);
    samples[8000] = 1.0;
    write_wav(&source, &samples, RATE, WavFormat::Float32);

    let model = LinearModel {
        scale: 1.0,
        offset: 0.0,
    };
    let options = RenderOptions {
        layout: Layout::SourceAligned,
        ..RenderOptions::default()
    };
    let out = dir.path().join("corrected.wav");
    let result = render_source_aligned(&source, &model, &out, &options).expect("render");

    assert!(
        !result.resampled,
        "a unity correction went through the resampler"
    );
    assert_eq!(
        result.frames,
        samples.len() as u64,
        "the unity render changed the length"
    );

    let mut reader = WavReader::open(&out).expect("open the render");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the render");
    for index in 0..frames {
        assert_eq!(
            planes[0][index], samples[index],
            "frame {index} was altered: {} against {}",
            planes[0][index], samples[index]
        );
    }
}

// ---------------------------------------------------------------------------
// P1-06 — the writer's temporary files
// ---------------------------------------------------------------------------

/// Two writers to the same destination must not share a temporary file.
///
/// The temporary name used to be the destination's name plus the process id,
/// so two exports in one process landed on one temporary file and one of them
/// destroyed the other's samples.
#[test]
fn two_writers_to_the_same_destination_do_not_clash() {
    let dir = TempDir::new().expect("temp dir");
    let out = dir.path().join("same.wav");

    let mut first = WavWriter::create(&out, RATE, 1, WavFormat::Float32).expect("first writer");
    let mut second = WavWriter::create(&out, RATE, 1, WavFormat::Float32).expect("second writer");

    first.write_planar(&[vec![0.25f32; 1000]]).expect("write");
    second.write_planar(&[vec![0.75f32; 1000]]).expect("write");

    first.finalize().expect("finalise the first");
    second.finalize().expect("finalise the second");

    // The second writer's samples are the ones that survive, and they are whole.
    let mut reader = WavReader::open(&out).expect("open the result");
    let frames = reader.frames() as usize;
    let planes = reader.read_frames(0, frames).expect("read the result");
    assert_eq!(frames, 1000, "the file holds the wrong number of frames");
    assert!(
        planes[0].iter().all(|v| (*v - 0.75).abs() < 1e-6),
        "the file holds the wrong writer's samples"
    );
}

/// A writer that is abandoned must leave nothing at the destination, and no
/// temporary file beside it.
#[test]
fn an_abandoned_writer_leaves_nothing_behind() {
    let dir = TempDir::new().expect("temp dir");
    let out = dir.path().join("abandoned.wav");
    {
        let mut writer = WavWriter::create(&out, RATE, 1, WavFormat::Float32).expect("create");
        writer.write_planar(&[vec![0.5f32; 100]]).expect("write");
        // The writer is dropped without being finalised.
    }
    assert!(!out.exists(), "the destination was created anyway");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read the directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the temporary file was left behind: {leftovers:?}"
    );
}

// ---------------------------------------------------------------------------
// P1-07 — the size limit
// ---------------------------------------------------------------------------

/// An output that cannot be represented as an ordinary RIFF file must be
/// refused, not written with a size field that has wrapped.
#[test]
fn an_output_that_does_not_fit_in_a_wav_file_is_refused() {
    let frames = RIFF_MAX_DATA_BYTES / 4 + 1024;
    let error = ensure_wav_size_fits(frames, 2, WavFormat::Float32)
        .expect_err("an oversized output must be refused");
    let message = format!("{error:#}");
    assert!(
        message.contains("RIFF"),
        "the refusal does not say what is wrong: {message}"
    );

    // A file that does fit is accepted.
    ensure_wav_size_fits(1024, 2, WavFormat::Float32).expect("a small output must be accepted");
}

// ---------------------------------------------------------------------------
// P1-01 — the export must not land on a recording
// ---------------------------------------------------------------------------

/// A sidecar must not overwrite a recording that happens to have the same name.
///
/// The audit's reproduction: a recording is called `syncmap.csv`, the export
/// directory is the directory the recording is in — which is the ordinary case —
/// and the CSV sidecar lands on the recording. The sidecars are files too, and
/// the export has to check them against the recordings just as it checks the
/// corrected audio.
#[test]
fn a_sidecar_cannot_overwrite_a_recording_of_the_same_name() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);

    // A recording whose *name* is one of the sidecar names.
    let recording = dir.path().join("syncmap.csv");
    write_wav(
        &recording,
        &sine(RATE, DURATION, 440.0, 0.4),
        RATE,
        WavFormat::Float32,
    );
    let before = std::fs::read(&recording).expect("read the recording");

    let report = report(
        &reference,
        vec![alignment(&recording, LinearModel::new(1.0, 0.0), DURATION)],
        Vec::new(),
        DURATION,
    );
    let inputs: Vec<std::path::PathBuf> = std::iter::once(report.reference.path.clone())
        .chain(report.sources.iter().map(|a| a.source.path.clone()))
        .collect();

    // The sidecar is part of the manifest, and the manifest is refused because
    // one of its files would land on the recording.
    let planned = stagesync_core::render::complete_outputs(
        dir.path(),
        &stagesync_core::render::planned_outputs(dir.path(), &report, Layout::SourceAligned),
    );
    assert!(
        planned.iter().any(|p| p.ends_with("syncmap.csv")),
        "the sidecar is not in the manifest: {planned:?}"
    );

    let error = stagesync_core::render::ensure_outputs_avoid_inputs(&planned, &inputs)
        .expect_err("the sidecar must not land on the recording");
    let message = format!("{error:#}");
    assert!(
        message.contains("syncmap.csv"),
        "the refusal does not name the file: {message}"
    );

    // The recording is untouched.
    assert_eq!(
        std::fs::read(&recording).expect("read the recording"),
        before,
        "the recording was modified"
    );
}

/// The corrected audio and the sidecars are checked by the same rule.
///
/// The audio and the sidecars are one manifest. If only one of them were
/// checked, the guard would still let a recording through under the other
/// name.
#[test]
fn the_corrected_audio_and_the_sidecars_share_one_guard() {
    let dir = TempDir::new().expect("temp dir");
    let reference = dir.path().join("reference.wav");
    write_wav(
        &reference,
        &sine(RATE, DURATION, 330.0, 0.4),
        RATE,
        WavFormat::Float32,
    );

    // A source recording. Its corrected audio is written next to it, under the
    // name `song_corrected.wav`.
    let source = dir.path().join("song.wav");
    write_wav(
        &source,
        &sine(RATE, DURATION, 440.0, 0.4),
        RATE,
        WavFormat::Float32,
    );

    // A *different* recording that already has that name. The export would
    // destroy it, and the recordings are in the export directory because that
    // is where they usually are.
    let other = dir.path().join("song_corrected.wav");
    write_wav(
        &other,
        &sine(RATE, DURATION, 220.0, 0.4),
        RATE,
        WavFormat::Float32,
    );
    let before = std::fs::read(&other).expect("read the recording");

    let report = report(
        &reference,
        vec![alignment(&source, LinearModel::new(1.0, 0.0), DURATION)],
        Vec::new(),
        DURATION,
    );
    let mut inputs: Vec<std::path::PathBuf> = std::iter::once(report.reference.path.clone())
        .chain(report.sources.iter().map(|a| a.source.path.clone()))
        .collect();
    inputs.push(other.clone());

    let planned = stagesync_core::render::complete_outputs(
        dir.path(),
        &stagesync_core::render::planned_outputs(dir.path(), &report, Layout::SourceAligned),
    );
    assert!(
        planned.iter().any(|p| p.ends_with("song_corrected.wav")),
        "the corrected audio is not in the manifest: {planned:?}"
    );

    let error = stagesync_core::render::ensure_outputs_avoid_inputs(&planned, &inputs)
        .expect_err("the corrected audio must not land on the recording");
    assert!(
        format!("{error:#}").contains("song_corrected.wav"),
        "the refusal does not name the file: {error:#}"
    );

    // The recording is untouched.
    assert_eq!(
        std::fs::read(&other).expect("read the recording"),
        before,
        "the recording was modified"
    );
}

// ---------------------------------------------------------------------------
// P2-02 — the reader
// ---------------------------------------------------------------------------

/// The reader must report frames, not channel samples.
///
/// A stereo file holds twice as many samples as it has frames, and the reader
/// reported the sample count, so every caller that wanted frames was working
/// with a number twice too large.
#[test]
fn the_wav_reader_counts_frames_and_not_samples() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("stereo.wav");
    let left = sine(RATE, 1.0, 440.0, 0.5);
    let right = sine(RATE, 1.0, 660.0, 0.5);
    write_wav_stereo_file(&path, &left, &right, RATE);

    let reader = WavReader::open(&path).expect("open");
    assert_eq!(reader.channels(), 2);
    assert_eq!(
        reader.frames(),
        left.len() as u64,
        "the frame count is not the frame count"
    );
}

/// A 24-bit file must be read at its own scale.
///
/// The full scale of a 24-bit sample is 2^23, not 2^31. Dividing by the 32-bit
/// full scale put every sample 48 dB too low, which made the verifier read a
/// 24-bit recording as if it were almost silent.
#[test]
fn a_24_bit_wav_is_read_at_its_own_scale() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("twentyfour.wav");
    // Half of the 24-bit full scale.
    let half = 4_194_304i32;
    write_24_bit_pcm(&path, &[half, -half, half, -half]);

    let mut reader = WavReader::open(&path).expect("open");
    assert_eq!(reader.channels(), 1);
    assert_eq!(reader.frames(), 4);
    let planes = reader.read_frames(0, 4).expect("read");
    assert!(
        (planes[0][0] - 0.5).abs() < 1e-6,
        "half scale read as {}, not 0.5",
        planes[0][0]
    );
    assert!(
        (planes[0][1] + 0.5).abs() < 1e-6,
        "negative half scale read as {}, not -0.5",
        planes[0][1]
    );
}

// ---------------------------------------------------------------------------
// P1-08 — the normalisation must not depend on where the silence is
// ---------------------------------------------------------------------------

/// A recording whose silence falls in a different place from the reference's
/// must still be aligned.
///
/// The source and the reference are two recorders in two places in the room,
/// and they do not agree on where the quiet passages are. If the normalisation
/// depended on the silence, the two files would get different shapes wherever
/// the silence differed, and that difference is a bias in the measured drift.
#[test]
fn a_recording_whose_silence_is_elsewhere_is_still_aligned() {
    use common::{analyse, config, make_source, SourceSpec};

    let dir = TempDir::new().expect("temp dir");
    let reference_samples = reference_signal(RATE, 240.0, 0xA5A5);

    // The reference is silent for five seconds in the middle.
    let mut reference_with_hole = reference_samples.clone();
    for sample in reference_with_hole
        .iter_mut()
        .take(120 * RATE as usize)
        .skip(60 * RATE as usize)
    {
        *sample = 0.0;
    }
    let reference_path = dir.path().join("reference.wav");
    write_wav(
        &reference_path,
        &reference_with_hole,
        RATE,
        WavFormat::Float32,
    );

    // The source starts ten seconds in, and its recorder heard nothing for
    // five seconds at a moment when the reference was playing: the silence is
    // in a different place in each file.
    let spec = SourceSpec {
        scale: 1.0,
        offset: 10.0,
        out_rate: RATE,
        gain_db: -12.0,
        duration_secs: None,
    };
    let (mut samples, _) = make_source(&reference_with_hole, RATE, spec, 0x1234);
    let hole_start = 100 * RATE as usize;
    let hole_end = 105 * RATE as usize;
    for sample in samples.iter_mut().take(hole_end).skip(hole_start) {
        *sample = 0.0;
    }
    let source = dir.path().join("source.wav");
    write_wav(&source, &samples, RATE, WavFormat::Float32);

    let report = analyse(&reference_path, std::slice::from_ref(&source), &config());
    let model = report.sources[0]
        .model()
        .expect("the source should have been aligned");
    let offset_error_ms = (model.offset - 10.0) * 1000.0;
    assert!(
        offset_error_ms.abs() < 15.0,
        "the offset came out {offset_error_ms:.3} ms from the truth of 10 s"
    );

    // The drift must be reported with the uncertainty that produced it, and
    // the figure must be small: the fixture carries no drift at all.
    let drift = report.sources[0]
        .drift
        .as_ref()
        .expect("the drift must be reported with its uncertainty");
    assert!(
        drift.drift_ppm.abs() < 60.0,
        "the drift came out at {} ppm on a fixture with none",
        drift.drift_ppm
    );
    assert!(
        drift.slope_se_ppm.is_finite(),
        "the drift was reported without a usable uncertainty"
    );
}

/// The normalisation must not flatten the music that sits next to a silence.
///
/// This is the property the matcher depends on. The correlation is invariant to
/// an affine change of either side, so what matters is that the *music* keeps
/// its own dynamic range: if the statistics of the window were taken from
/// every frame, a silence in the window would inflate the spread and squash
/// the music, and the reference — which has no silence at that position —
/// would not match it.
///
/// The test was written after the alternative was measured: with the
/// all-frame statistics, a recording with a recorder gap had its measured
/// correspondence move by up to 6.5 seconds and every probe became
/// untrustworthy.
#[test]
fn the_normalisation_keeps_the_music_beside_a_silence_intact() {
    use stagesync_core::features::envelope::local_zscore_masked;

    // Music with a constant dynamic range: it alternates between two levels, so
    // the spread of its normalised values says how much of that range survived.
    let music: Vec<f32> = (0..401).map(|i| 20.0 + 10.0 * (i % 2) as f32).collect();

    // The same music, with ten frames of deep silence in the middle.
    let mut with_silence = music.clone();
    for value in with_silence.iter_mut().take(210).skip(200) {
        *value = -100.0;
    }
    let mut quiet = vec![false; with_silence.len()];
    for flag in quiet.iter_mut().take(210).skip(200) {
        *flag = true;
    }

    let clean = local_zscore_masked(&music, 50, 4.0, &[]);
    let masked = local_zscore_masked(&with_silence, 50, 4.0, &quiet);

    // The spread of the normalised values.
    let spread = |values: &[f32], from: usize, to: usize| -> f32 {
        let slice = &values[from..to];
        slice.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - slice.iter().copied().fold(f32::INFINITY, f32::min)
    };

    // Away from the silence the two agree.
    let away_clean = spread(&clean, 50, 150);
    let away_masked = spread(&masked, 50, 150);
    assert!(
        (away_clean - away_masked).abs() < 1e-3,
        "the normalisation changed away from the silence: {away_masked} against {away_clean}"
    );

    // Beside the silence the music must keep its dynamic range. If the
    // statistics of the window were taken from every frame, the silence would
    // inflate the spread and squash the music, and the reference — which has no
    // silence at that position — would not match it.
    let beside_clean = spread(&clean, 150, 200);
    let beside_masked = spread(&masked, 150, 200);
    assert!(
        beside_masked > beside_clean * 0.8,
        "the silence beside the music flattened it: {beside_masked} against {beside_clean}"
    );
}

// ---------------------------------------------------------------------------
// P2-01 — the manual anchor is an override, not a hint
// ---------------------------------------------------------------------------

/// A manual anchor must survive the drift assessment.
///
/// The anchor used to be applied to the envelope model only. When the drift
/// assessment kept the *waveform* model — usually the better one — the anchor
/// was quietly discarded, so the placement the user asked for was not the
/// placement that was used.
#[test]
fn a_manual_anchor_survives_the_drift_assessment() {
    use common::{analyse, config, make_source, SourceSpec};
    use stagesync_core::pipeline::{analyze, ManualAnchor};

    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, 240.0, 0xBEEF);
    let reference_samples = reference_signal(RATE, 240.0, 0xBEEF);
    let (samples, _) = make_source(
        &reference_samples,
        RATE,
        SourceSpec {
            scale: 1.0 + 35.0 / 1e6,
            offset: 12.0,
            out_rate: RATE,
            gain_db: -10.0,
            duration_secs: None,
        },
        0xF00D,
    );
    let source = dir.path().join("source.wav");
    write_wav(&source, &samples, RATE, WavFormat::Float32);

    // The automatic match puts the source twelve seconds in. The user says it
    // belongs half a second later.
    let anchor = ManualAnchor {
        source_time: 30.0,
        reference_time: 42.5,
    };
    let mut analysis = config();
    analysis.manual_anchors = vec![(source.to_string_lossy().to_string(), anchor)];

    let mut progress = |_: stagesync_core::pipeline::Progress| {};
    let report = analyze(
        &reference,
        std::slice::from_ref(&source),
        &analysis,
        &mut progress,
    )
    .expect("the analysis");

    let alignment = &report.sources[0];
    let model = alignment.model().expect("a model");
    let placed = model.reference_time(anchor.source_time);
    assert!(
        (placed - anchor.reference_time).abs() < 1e-6,
        "the anchor was not applied: the model places the source at {placed}, not {}",
        anchor.reference_time
    );

    // And the override is recorded, with how far it moved the placement.
    let state = alignment
        .override_state
        .expect("the override must be recorded in the report");
    assert_eq!(state.anchor_source_time, anchor.source_time);
    assert_eq!(state.anchor_reference_time, anchor.reference_time);
    assert!(
        state.shift_ms.abs() > 100.0,
        "the override did not move the placement: {}",
        state.shift_ms
    );
    assert!(
        alignment
            .warnings
            .iter()
            .any(|w| w.contains("manual anchor")),
        "the override is not mentioned in the warnings: {:?}",
        alignment.warnings
    );

    // The same analysis without the anchor finds the automatic placement, so
    // the test above is measuring the override and not the match.
    let plain = analyse(&reference, std::slice::from_ref(&source), &config());
    let plain_model = plain.sources[0].model().expect("a model");
    assert!(
        plain.sources[0].override_state.is_none(),
        "an override was recorded without an anchor"
    );
    assert!(
        (plain_model.offset - 12.0).abs() < 0.05,
        "the automatic offset is {}, not the expected 12 s",
        plain_model.offset
    );
}

/// An anchor belongs to one recording, and two recordings that share a file
/// name are two different recordings.
#[test]
fn an_anchor_is_keyed_by_the_recording_and_not_by_its_name() {
    use stagesync_core::audio::AudioDecoder;
    use stagesync_core::pipeline::AnalysisConfig;

    let dir = TempDir::new().expect("temp dir");
    let first = dir.path().join("song.wav");
    let second = dir.path().join("other").join("song.wav");
    write_wav(
        &first,
        &sine(RATE, 1.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );
    std::fs::create_dir_all(second.parent().unwrap()).expect("create the folder");
    write_wav(
        &second,
        &sine(RATE, 1.0, 440.0, 0.5),
        RATE,
        WavFormat::Float32,
    );

    let anchor = stagesync_core::pipeline::ManualAnchor {
        source_time: 0.0,
        reference_time: 1.0,
    };
    // Keyed by the bare name: this must not match either recording.
    let config = AnalysisConfig {
        manual_anchors: vec![("song.wav".to_string(), anchor)],
        ..AnalysisConfig::default()
    };
    assert!(config
        .anchor_for(AudioDecoder::open(&first).unwrap().meta())
        .is_none());

    // Keyed by the full path: it matches that recording and only that one.
    let config = AnalysisConfig {
        manual_anchors: vec![(first.to_string_lossy().to_string(), anchor)],
        ..AnalysisConfig::default()
    };
    assert!(config
        .anchor_for(AudioDecoder::open(&first).unwrap().meta())
        .is_some());
    assert!(config
        .anchor_for(AudioDecoder::open(&second).unwrap().meta())
        .is_none());
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a stereo WAV file.
fn write_wav_stereo_file(path: &Path, left: &[f32], right: &[f32], rate: u32) {
    let frames = left.len().min(right.len());
    let mut writer = WavWriter::create(path, rate, 2, WavFormat::Float32).expect("create");
    writer
        .write_planar(&[left[..frames].to_vec(), right[..frames].to_vec()])
        .expect("write");
    writer.finalize().expect("finalise");
}

/// Write a 24-bit PCM WAV file by hand.
///
/// The test cannot use the float writer: a 24-bit file is a different format
/// on disk, and the reader has to be able to read one.
fn write_24_bit_pcm(path: &Path, samples: &[i32]) {
    let mut data = Vec::with_capacity(samples.len() * 3);
    for sample in samples {
        let value = *sample;
        data.push((value & 0xFF) as u8);
        data.push(((value >> 8) & 0xFF) as u8);
        data.push(((value >> 16) & 0xFF) as u8);
    }
    let byte_rate = RATE * 3;
    let block_align: u16 = 3;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    std::fs::write(path, bytes).expect("write the 24-bit file");
}
