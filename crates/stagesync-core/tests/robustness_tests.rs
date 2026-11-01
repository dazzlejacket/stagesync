//! Robustness tests: bad input, cancellation, long recordings, compressed formats.
//!
//! A synchroniser that only works on clean input is not a synchroniser. These tests
//! check that bad material produces an *error or an honest report*, never a panic and
//! never a confident wrong answer.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use common::{
    config, make_source, reference_file, reference_signal, write_wav, GroundTruth, SourceSpec,
};
use stagesync_core::audio::{AudioDecoder, WavFormat, WavWriter};
use stagesync_core::pipeline::{analyze, AnalysisConfig, CancelToken, Progress, CANCELLED};
use tempfile::TempDir;

const RATE: u32 = 8000;
const SOURCE_RATE: u32 = 7200;
/// The rate the compressed test's source is written at.
///
/// The AAC encoder this test uses only accepts a fixed list of sample rates, so
/// a source at 7200 Hz would have to be resampled by the encoder before it was
/// encoded. The resampling is itself a large degradation, and it is not what
/// this test is about: the test is about the *compressed* path. The source is
/// therefore written at a rate the encoder accepts as it is, so the encode
/// changes nothing but the compression.
const COMPRESSED_RATE: u32 = 44100;
const DURATION: f64 = 120.0;

/// Run the analysis and return the result, without panicking on a failure.
fn try_analyse(
    reference: &Path,
    sources: &[PathBuf],
    cfg: &AnalysisConfig,
) -> anyhow::Result<stagesync_core::matching::AlignmentReport> {
    let mut progress = |_p: Progress| {};
    analyze(reference, sources, cfg, &mut progress)
}

/// Build a source file from the shared reference.
fn build_source(dir: &TempDir, name: &str, duration_secs: Option<f64>) -> (PathBuf, GroundTruth) {
    build_source_at(dir, name, duration_secs, SOURCE_RATE)
}

/// Build a source file from the shared reference, at a chosen rate.
fn build_source_at(
    dir: &TempDir,
    name: &str,
    duration_secs: Option<f64>,
    out_rate: u32,
) -> (PathBuf, GroundTruth) {
    let rs = reference_signal(RATE, DURATION, 0xC0FFEE);
    let (samples, truth) = make_source(
        &rs,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 20.0,
            out_rate,
            gain_db: -20.0,
            duration_secs,
        },
        0xBEEF,
    );
    let path = dir.path().join(name);
    write_wav(&path, &samples, out_rate, WavFormat::Float32);
    (path, truth)
}

#[test]
fn a_file_that_is_not_audio_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let junk = dir.path().join("junk.wav");
    std::fs::write(&junk, b"this is definitely not a wave file at all").expect("write");

    let result = try_analyse(&reference, std::slice::from_ref(&junk), &config());
    assert!(
        result.is_err(),
        "a file that is not audio should be rejected, got {:?}",
        result.map(|r| r.sources.len())
    );
}

#[test]
fn an_empty_file_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let empty = dir.path().join("empty.wav");
    std::fs::write(&empty, b"").expect("write");

    let result = try_analyse(&reference, std::slice::from_ref(&empty), &config());
    assert!(result.is_err(), "an empty file should be rejected");
}

#[test]
fn a_missing_file_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let missing = dir.path().join("does-not-exist.wav");

    let result = try_analyse(&reference, &[missing], &config());
    assert!(result.is_err(), "a missing file should be rejected");
}

#[test]
fn a_truncated_wav_does_not_crash_the_pipeline() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    build_source(&dir, "whole.wav", None);

    // A well-formed file, cut in half.
    let whole = dir.path().join("whole.wav");
    let bytes = std::fs::read(&whole).expect("read");
    let truncated = dir.path().join("truncated.wav");
    std::fs::write(&truncated, &bytes[..bytes.len() / 2]).expect("write");

    // Whether the truncated file decodes or not, the pipeline must answer rather than
    // panic.
    let result = try_analyse(&reference, &[truncated], &config());
    if let Ok(report) = &result {
        // If it did decode, the alignment must at least be reported honestly.
        for alignment in &report.sources {
            assert!(
                alignment.direct_model.is_some() || !alignment.warnings.is_empty(),
                "a source that could not be matched must carry a warning"
            );
        }
    }
}

#[test]
fn a_source_shorter_than_the_matching_window_is_reported_not_panicked() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, _) = build_source(&dir, "tiny.wav", Some(2.0));

    let report = try_analyse(&reference, &[source], &config()).expect("the analysis should answer");
    let alignment = &report.sources[0];
    assert!(
        alignment.points.is_empty(),
        "a two second source cannot be matched to a minute of material"
    );
    assert!(
        !alignment.warnings.is_empty(),
        "an unmatched source must say so"
    );
}

#[test]
fn cancelling_before_the_analysis_stops_it() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, _) = build_source(&dir, "src.wav", None);

    let token = CancelToken::new();
    token.cancel();
    let cfg = AnalysisConfig {
        cancel: Some(token),
        ..config()
    };

    let result = try_analyse(&reference, &[source], &cfg);
    let err = result.expect_err("a cancelled analysis must fail");
    assert!(
        format!("{err:#}").contains(CANCELLED),
        "the error should say the analysis was cancelled, got {err:#}"
    );
}

#[test]
fn cancelling_during_the_analysis_stops_it_promptly() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (source, _) = build_source(&dir, "src.wav", None);

    let token = CancelToken::new();
    let cancelling = token.clone();
    let mut progress = move |_p: Progress| {
        cancelling.cancel();
    };
    let cfg = AnalysisConfig {
        cancel: Some(token),
        ..config()
    };

    let result = analyze(&reference, &[source], &cfg, &mut progress);
    let err = result.expect_err("a cancelled analysis must fail");
    assert!(
        format!("{err:#}").contains(CANCELLED),
        "the error should say the analysis was cancelled, got {err:#}"
    );
}

#[test]
fn a_compressed_recording_is_matched_like_an_uncompressed_one() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    let (wav, truth) = build_source_at(&dir, "src.wav", None, COMPRESSED_RATE);

    // Encode to AAC in an MP4 container, the way a watch recorder writes a file.
    //
    // The channel layout is stated on the command line because the project's own
    // float WAV files declare `WAVE_FORMAT_EXTENSIBLE` with a **zero channel
    // mask**, which the specification leaves unspecified and which ffmpeg's
    // demuxer therefore reports as an unknown layout. ffmpeg's AAC encoder
    // refuses a stream whose layout it does not know — it fails with
    // "Unsupported channel layout" — while its other encoders accept it. The
    // file's data is mono, so the layout is stated as mono here. This is a
    // property of the writer, not of the material, and it is recorded in
    // `FINAL_REPAIR_HANDOFF.md`.
    let m4a = dir.path().join("src.m4a");
    let encoded = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-channel_layout",
            "mono",
            "-i",
        ])
        .arg(&wav)
        .args(["-c:a", "aac", "-b:a", "128k"])
        .arg(&m4a)
        .status();
    // A failed encode must not be reported as a pass. The test either checks
    // the compressed path or it says plainly that the compressed path was not
    // checked; it never returns quietly.
    match encoded {
        Ok(status) if status.success() => {}
        Ok(status) => panic!(
            "SKIP-UNVERIFIED: ffmpeg exited with {status} and did not produce the AAC file, so \
             the compressed-format path was not verified"
        ),
        Err(err) => panic!(
            "SKIP-UNVERIFIED: ffmpeg could not be run ({err}), so the compressed-format path was \
             not verified"
        ),
    }
    // The encode has to have produced a file, and not an empty one: an empty
    // file decodes as silence, and the assertions below would then pass for
    // the wrong reason.
    let size = std::fs::metadata(&m4a)
        .expect("the encoded file should exist")
        .len();
    assert!(
        size > 1024,
        "SKIP-UNVERIFIED: the encoded AAC file is only {size} bytes, so the compressed-format \
         path was not verified"
    );

    let report = try_analyse(&reference, std::slice::from_ref(&m4a), &config())
        .expect("the analysis should work");
    let alignment = &report.sources[0];
    let errors: Vec<f64> = alignment
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.reference_time - truth.reference_time(p.source_time)) * 1000.0)
        .collect();
    assert!(
        errors.len() >= 5,
        "expected at least 5 confident positions, got {}",
        errors.len()
    );
    let mut sorted = errors.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[sorted.len() / 2].abs();
    // The same material, uncompressed, for comparison.
    let plain = try_analyse(&reference, std::slice::from_ref(&wav), &config())
        .expect("the analysis should work");
    let plain_errors: Vec<f64> = plain.sources[0]
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.reference_time - truth.reference_time(p.source_time)) * 1000.0)
        .collect();
    assert!(
        plain_errors.len() >= 5,
        "expected at least 5 confident positions in the uncompressed recording, got {}",
        plain_errors.len()
    );
    let mut plain_sorted = plain_errors.clone();
    plain_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let plain_p50 = plain_sorted[plain_sorted.len() / 2].abs();
    assert!(
        plain_p50 <= 10.0,
        "the uncompressed recording was already misaligned: p50 error {plain_p50:.3} ms"
    );

    // **The property under test.** The compressed recording has to be matched
    // like the uncompressed one: the two analyses have to agree on *where* the
    // material is, to within the same precision.
    //
    // They will not agree on the *absolute* time, because an AAC encoder
    // primes the stream with 1024 samples and the decoder does not take them
    // back out again: the compressed recording starts 1024 samples late, which
    // is 23 ms at this rate. That is a property of the codec, it is the same
    // for every probe, and it is not something the matcher can undo. What the
    // matcher *can* do is place the material as precisely as it places the
    // uncompressed material, so the difference between the two analyses has to
    // be one constant.
    let compressed: Vec<(f64, f64)> = alignment
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.source_time, p.reference_time))
        .collect();
    let uncompressed: Vec<(f64, f64)> = plain.sources[0]
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.source_time, p.reference_time))
        .collect();
    // The two analyses probe the same positions, but not always the same
    // *number* of them, so the probes are paired by their position in the
    // source rather than by their order.
    let mut differences: Vec<f64> = compressed
        .iter()
        .filter_map(|(t, c)| {
            uncompressed
                .iter()
                .find(|(u, _)| (u - t).abs() < 1e-6)
                .map(|(_, u)| (c - u) * 1000.0)
        })
        .collect();
    assert!(
        differences.len() >= 5,
        "only {} probe(s) were trusted in both analyses",
        differences.len()
    );
    differences.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = differences[differences.len() / 2];
    let spread = differences[differences.len() - 1] - differences[0];
    assert!(
        spread <= 10.0,
        "the compressed recording was matched with a spread of {spread:.3} ms around a median \
         offset of {median:.3} ms, where the uncompressed one was matched to \
         {plain_p50:.3} ms"
    );
    // The constant offset is the codec's priming delay and nothing else, so it
    // has to be small: 1024 samples at 44.1 kHz is 23 ms.
    assert!(
        median.abs() <= 50.0,
        "the compressed recording was matched {median:.3} ms away from the uncompressed one, \
         which is more than the codec's priming delay"
    );
    // And the compressed recording must not be worse than the truth by more
    // than the codec's delay either.
    assert!(
        p50 <= 50.0,
        "p50 error {p50:.3} ms is too large for a lossy source"
    );
}

#[test]
fn a_cancellation_token_can_be_shared_between_threads() {
    let flag = Arc::new(AtomicBool::new(false));
    let token = CancelToken::from_flag(flag.clone());
    assert!(!token.is_cancelled());
    flag.store(true, Ordering::SeqCst);
    assert!(token.is_cancelled());
}

/// Write a long recording straight to disk, without ever holding it in memory.
///
/// The reference is generated by the shared fixture generator, which produces material
/// whose envelope never repeats exactly. The *source* is then built by streaming the
/// reference's second hour, so the test itself never holds an hour of samples.
fn build_long_pair(dir: &TempDir) -> (PathBuf, PathBuf) {
    const LONG_RATE: u32 = 4000;
    const HOURS: f64 = 2.0;

    let reference = dir.path().join("long.wav");
    let signal = reference_signal(LONG_RATE, HOURS * 3600.0, 0xC0FFEE);
    write_wav(&reference, &signal, LONG_RATE, WavFormat::Float32);
    drop(signal);

    let source = dir.path().join("long_source.wav");
    {
        let mut reader = AudioDecoder::open(&reference).expect("open");
        let mut writer =
            WavWriter::create(&source, LONG_RATE, 1, WavFormat::Float32).expect("create");

        // Skip the first hour.
        let hour = (3600.0 * LONG_RATE as f64) as u64;
        let mut to_skip = hour;
        while to_skip > 0 {
            let want = to_skip.min(1 << 16) as usize;
            let Some(block) = reader.read_block(want).expect("read") else {
                break;
            };
            to_skip = to_skip.saturating_sub(block.planes[0].len() as u64);
        }

        // Write the second hour, with a level change.
        let mut remaining = hour;
        while remaining > 0 {
            let want = remaining.min(1 << 16) as usize;
            let Some(block) = reader.read_block(want).expect("read") else {
                break;
            };
            let plane: Vec<f32> = block.planes[0].iter().map(|v| v * 0.1).collect();
            let frames = plane.len() as u64;
            writer.write_planar(&[plane]).expect("write");
            remaining = remaining.saturating_sub(frames);
        }
        writer.finalize().expect("finalize");
    }
    (reference, source)
}

#[test]
fn a_two_hour_recording_is_handled_without_loading_it_whole() {
    let dir = TempDir::new().expect("temp dir");
    let (reference, source) = build_long_pair(&dir);

    let size = std::fs::metadata(&reference).expect("stat").len();
    assert!(
        size > 100_000_000,
        "the long fixture should be well over 100 MB, got {size}"
    );

    {
        let reader = AudioDecoder::open(&reference).expect("open");
        let meta = reader.meta().clone();
        assert_eq!(meta.sample_rate, 4000);
        assert_eq!(meta.channels, 1);
        assert!(
            (meta.duration_secs - 7200.0).abs() < 5.0,
            "the fixture is not two hours long: {}",
            meta.duration_secs
        );
    }
    {
        let reader = AudioDecoder::open(&source).expect("open");
        let meta = reader.meta().clone();
        assert!(
            (meta.duration_secs - 3600.0).abs() < 5.0,
            "the source should be one hour long: {}",
            meta.duration_secs
        );
    }

    let baseline = peak_working_set_bytes();
    let mut progress = |_p: Progress| {};
    let report = analyze(
        &reference,
        std::slice::from_ref(&source),
        &config(),
        &mut progress,
    )
    .expect("analysis");
    let peak = peak_working_set_bytes();

    let alignment = &report.sources[0];
    assert!(
        !alignment.points.is_empty(),
        "a two hour recording should still be matched"
    );
    let errors: Vec<f64> = alignment
        .points
        .iter()
        .filter(|p| p.confidence.is_trustworthy())
        .map(|p| (p.reference_time - (p.source_time + 3600.0)) * 1000.0)
        .collect();
    assert!(!errors.is_empty(), "no confident positions were found");
    let mut sorted = errors.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[sorted.len() / 2].abs();
    assert!(
        p50 <= 10.0,
        "p50 error {p50:.3} ms is too large over two hours"
    );

    if let (Some(before), Some(after)) = (baseline, peak) {
        let growth = after.saturating_sub(before);
        // The file is over 100 MB. If the pipeline loaded it into memory the working set
        // would grow by at least that; streaming it must cost a fraction of that.
        assert!(
            growth < size / 2,
            "the analysis grew the working set by {growth} bytes for a {size} byte file; \
             it should stream the file rather than load it"
        );
    }
}

/// The peak working set of this process, in bytes.
///
/// Only Windows is measured; on other platforms the memory check is skipped rather
/// than guessed at.
#[cfg(windows)]
fn peak_working_set_bytes() -> Option<u64> {
    use std::ffi::c_void;
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: the current process handle is always valid and the struct is sized
    // correctly for the call.
    let ok = unsafe {
        GetProcessMemoryInfo(
            std::ptr::null_mut(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        None
    } else {
        Some(counters.peak_working_set_size as u64)
    }
}

#[cfg(not(windows))]
fn peak_working_set_bytes() -> Option<u64> {
    None
}

/// Write a WAV file whose header claims the given sample rate.
///
/// The header is written by hand, because a sample rate of zero cannot be
/// asked for through the usual writer.
fn write_wav_with_sample_rate(path: &Path, sample_rate: u32, frames: usize) {
    let mut data = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let value = (i % 37) as i16 * 400;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the directory");
    }
    std::fs::write(path, bytes).expect("write the file");
}

/// A file that claims a sample rate of zero is refused, with a clear message.
///
/// Nothing can be measured against a file whose clock does not run.
#[test]
fn a_file_with_no_sample_rate_is_refused() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("no_rate.wav");
    write_wav_with_sample_rate(&path, 0, 800);

    let opened = AudioDecoder::open(&path);
    assert!(
        opened.is_err(),
        "a file with no sample rate must be refused"
    );
    let error = opened.err().expect("the file must be refused");
    let message = format!("{error:#}");
    assert!(
        message.contains("sample rate of zero"),
        "the refusal must say why, got: {message}"
    );
}

/// A very low sample rate must not crash the analysis.
///
/// The lowest band of the mel filterbank can fall above the highest FFT bin
/// when the sample rate is low, and the band then had nowhere to read from.
#[test]
fn a_very_low_sample_rate_does_not_crash_the_analysis() {
    let dir = TempDir::new().expect("temp dir");
    let reference = reference_file(RATE, DURATION, 0xC0FFEE);
    for rate in [1u32, 8, 40, 77, 78, 100] {
        let path = dir.path().join(format!("low_{rate}.wav"));
        write_wav_with_sample_rate(&path, rate, 800);

        // The spectral cross-check is the stage that builds the filterbank.
        let mut config = config();
        config.compute_log_mel = true;
        let result = try_analyse(&reference, std::slice::from_ref(&path), &config);
        assert!(
            result.is_ok(),
            "a sample rate of {rate} Hz must not fail the analysis: {:?}",
            result.err()
        );
    }
}
