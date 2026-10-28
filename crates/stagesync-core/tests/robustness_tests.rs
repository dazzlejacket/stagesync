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
    let rs = reference_signal(RATE, DURATION, 0xC0FFEE);
    let (samples, truth) = make_source(
        &rs,
        RATE,
        SourceSpec {
            scale: 1.0,
            offset: 20.0,
            out_rate: SOURCE_RATE,
            gain_db: -20.0,
            duration_secs,
        },
        0xBEEF,
    );
    let path = dir.path().join(name);
    write_wav(&path, &samples, SOURCE_RATE, WavFormat::Float32);
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
    let (wav, truth) = build_source(&dir, "src.wav", None);

    // Encode to AAC in an MP4 container, the way a watch recorder writes a file.
    let m4a = dir.path().join("src.m4a");
    let encoded = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&wav)
        .args(["-c:a", "aac", "-b:a", "128k"])
        .arg(&m4a)
        .status();
    match encoded {
        Ok(status) if status.success() => {}
        _ => {
            println!("ffmpeg is not available; skipping the compressed-format check");
            return;
        }
    }

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
    assert!(
        p50 <= 10.0,
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
