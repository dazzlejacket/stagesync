//! Integration checks for the parts of the desktop application that do not need a
//! window: the export step and the file-list bookkeeping.
//!
//! The export is the part of the application that writes to disk, so it is the part
//! that most needs an automated check: every artefact the window promises to produce
//! has to be there, at a real path, with real content.
//!
//! The reports used here are produced by the real pipeline and then adjusted through
//! their serialised form. That keeps the test independent of the exact field set of
//! the core's data model, which is the core's business and not this crate's.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use stagesync_core::audio::{WavFormat, WavReader, WavWriter};
use stagesync_core::matching::{AlignmentReport, LinearModel};
use stagesync_core::pipeline::{analyze, AnalysisConfig};
use stagesync_core::render::Layout;

use stagesync_desktop::jobs::{export_report, JobFailure};
use stagesync_desktop::state::{is_wav, metrics_for, stem, ExportSettings, TaskProgress};

/// The rate of the synthetic recordings.
const RATE: u32 = 16_000;
/// The length of the synthetic recordings.
///
/// The core's coarse stage correlates windows of sixty seconds, so a recording shorter
/// than that is outside what the pipeline supports; the fixtures are kept above it.
const SECONDS: f64 = 70.0;

/// A unique scratch directory for one test.
fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "stagesync-desktop-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("the scratch directory could be created");
    dir
}

/// Write a short deterministic mono WAV file.
fn write_test_wav(path: &Path, rate: u32, seconds: f64, seed: u64) {
    let mut writer =
        WavWriter::create(path, rate, 1, WavFormat::Float32).expect("the WAV was created");
    let frames = (rate as f64 * seconds) as usize;
    let mut state = seed;
    let mut samples = Vec::with_capacity(frames);
    for _ in 0..frames {
        // A xorshift generator: deterministic, broadband, and easy to reproduce.
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let value = ((state >> 33) as f32 / (1u64 << 30) as f32) - 1.0;
        samples.push(value * 0.2);
    }
    writer
        .write_planar(&[samples])
        .expect("the WAV was written");
    writer.finalize().expect("the WAV was finalised");
}

/// Analyse two synthetic recordings with the real pipeline.
///
/// The spectral cross-check is switched off: it is the core's, not this crate's, and
/// the test is about the workflow around it.
fn analyse_pair(reference: &Path, source: &Path) -> AlignmentReport {
    let config = AnalysisConfig {
        compute_log_mel: false,
        ..AnalysisConfig::default()
    };
    let mut progress = |_: stagesync_core::pipeline::Progress| {};
    analyze(
        reference,
        std::slice::from_ref(&source.to_path_buf()),
        &config,
        &mut progress,
    )
    .expect("the analysis of the synthetic pair succeeds")
}

/// Replace the fitted model of every source with a known one.
///
/// The report is edited through its serialised form so that the test does not depend
/// on the exact set of fields the core's data model carries.
///
/// The drift verdict has the final say on the model that is applied, so the model is
/// put there as well: a verdict that still carried the old model would be applied
/// instead of the one the test asks for.
fn with_model(report: &AlignmentReport, model: LinearModel) -> AlignmentReport {
    let mut value = serde_json::to_value(report).expect("the report serialises");
    {
        let sources = value
            .get_mut("sources")
            .and_then(|sources| sources.as_array_mut())
            .expect("the report has sources");
        let serialised = serde_json::to_value(model).expect("the model serialises");
        for source in sources {
            source["calibrated_model"] = serialised.clone();
            source["direct_model"] = serialised.clone();
            if let Some(drift) = source
                .get_mut("drift")
                .and_then(|drift| drift.as_object_mut())
            {
                drift.insert("model".to_string(), serialised.clone());
                drift.insert(
                    "drift_ppm".to_string(),
                    serde_json::json!(model.drift_ppm()),
                );
                drift.insert("status".to_string(), serde_json::json!("established"));
                drift.insert("method".to_string(), serde_json::json!("envelope"));
            }
        }
    }
    serde_json::from_value(value).expect("the report deserialises")
}

#[test]
fn export_writes_every_artefact_and_reports_real_paths() {
    let dir = scratch_dir("export");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 0x1234_5678);
    write_test_wav(&source, RATE, SECONDS, 0x8765_4321);

    let report = with_model(
        &analyse_pair(&reference, &source),
        LinearModel::new(1.0, 0.5),
    );

    let out = dir.join("out");
    let settings = ExportSettings {
        dir: out.display().to_string(),
        layout: Layout::SourceAligned,
        sample_rate: None,
        format: WavFormat::Float32,
    };

    let mut progress: Vec<TaskProgress> = Vec::new();
    let cancel = AtomicBool::new(false);
    let outcome = {
        let mut sink = |update: TaskProgress| progress.push(update);
        export_report(&report, &settings, &cancel, &mut sink)
    };
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(JobFailure::Failed(message)) => panic!("the export failed: {message}"),
        Err(JobFailure::Cancelled) => panic!("the export was cancelled unexpectedly"),
    };

    // Every artefact the window promises must exist, with content.
    for name in [
        "syncmap.csv",
        "segments.csv",
        "drift_graph.svg",
        "timeline.txt",
        "plan.json",
        "report.json",
        "source_corrected.wav",
    ] {
        let path = out.join(name);
        let size = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("{name} was not written: {error}"))
            .len();
        assert!(size > 0, "{name} is empty");
    }

    // The reported paths must be absolute, so the user can open them.
    assert_eq!(outcome.files.len(), 7);
    for path in &outcome.files {
        assert!(path.is_absolute(), "{} is not absolute", path.display());
        assert!(path.exists(), "{} does not exist", path.display());
    }

    // The source-aligned render must be handed back with its source index, so the
    // hold-out check can find it.
    assert_eq!(outcome.corrected.len(), 1);
    assert_eq!(outcome.corrected[0].0, 0);
    assert!(outcome.corrected[0].1.ends_with("source_corrected.wav"));

    // The corrected audio must keep the source's length: correcting a clock changes
    // the duration by the drift, and this source has none.
    let reader = WavReader::open(&outcome.corrected[0].1).expect("the corrected WAV can be read");
    let frames = reader.frames();
    let expected = SECONDS * RATE as f64;
    assert!(
        (frames as f64 - expected).abs() < expected * 0.05,
        "the corrected audio has {frames} frames, which is not {SECONDS} seconds"
    );

    // The progress must have been reported and must have finished at 1.0.
    assert!(!progress.is_empty(), "the export reported no progress");
    assert_eq!(
        progress.last().map(|p| p.fraction),
        Some(1.0),
        "the export did not finish at 100%"
    );

    // The timeline summary must name the reference and the source.
    let summary =
        std::fs::read_to_string(out.join("timeline.txt")).expect("timeline.txt is readable");
    assert!(
        summary.contains("source.wav"),
        "the summary does not mention the source"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_stops_when_it_is_cancelled() {
    let dir = scratch_dir("cancel");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 11);
    write_test_wav(&source, RATE, SECONDS, 22);

    let report = with_model(
        &analyse_pair(&reference, &source),
        LinearModel::new(1.0, 0.0),
    );

    let out = dir.join("out");
    let settings = ExportSettings {
        dir: out.display().to_string(),
        layout: Layout::SourceAligned,
        sample_rate: None,
        format: WavFormat::Int16,
    };

    // The flag is already set, so the export must refuse to start.
    let cancel = AtomicBool::new(true);
    let mut sink = |_: TaskProgress| {};
    let result = export_report(&report, &settings, &cancel, &mut sink);
    assert!(
        matches!(result, Err(JobFailure::Cancelled)),
        "a cancelled export must not write anything"
    );
    assert!(
        !out.join("source_corrected.wav").exists(),
        "a cancelled export must not leave a file behind"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_analysis_keeps_the_paths_the_user_supplied() {
    let dir = scratch_dir("paths");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 33);
    write_test_wav(&source, RATE, SECONDS, 44);

    let report = analyse_pair(&reference, &source);

    // The file list and the report must agree on which file is which, or the window
    // would show one file's numbers next to another file's name.
    assert_eq!(report.reference.path, reference);
    assert_eq!(report.sources.len(), 1);
    assert_eq!(report.sources[0].source.path, source);
    assert_eq!(report.sources[0].source.file_name, "source.wav");
    assert!(report.timeline_duration > 0.0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_measured_figures_come_from_the_report() {
    let dir = scratch_dir("metrics");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 55);
    write_test_wav(&source, RATE, SECONDS, 66);

    // A 500 ppm drift over seventy seconds accumulates thirty-five milliseconds.
    let report = with_model(
        &analyse_pair(&reference, &source),
        LinearModel::new(1.0005, 2.25),
    );
    let metrics = metrics_for(&report, 0);

    assert_eq!(metrics.offset_secs, Some(2.25));
    let drift = metrics.drift_ppm.expect("the drift is known");
    let accumulated = metrics.accumulated_drift_ms.expect("the drift is known");
    assert!((drift - 500.0).abs() < 1e-6, "the drift is {drift} ppm");
    assert!(
        (accumulated - 35.0).abs() < 1e-3,
        "the accumulated drift is {accumulated} ms"
    );
    assert!(metrics.coverage.is_some());

    // An index that does not exist must not panic.
    let missing = metrics_for(&report, 7);
    assert_eq!(missing.offset_secs, None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn output_names_are_derived_from_the_input_names() {
    assert_eq!(stem("watch recording.wav"), "watch_recording");
    assert_eq!(stem("already_clean.wav"), "already_clean");
    assert!(is_wav(Path::new("C:/music/track.WAV")));
    assert!(!is_wav(Path::new("C:/music/track.m4a")));
}

#[test]
fn a_cancelled_analysis_stops_and_reports_no_result() {
    let dir = scratch_dir("cancel-analysis");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 77);
    write_test_wav(&source, RATE, SECONDS, 88);

    // The same hook the application installs: the deliberate cancellation unwind
    // must not be reported as a crash.
    stagesync_desktop::jobs::install_panic_hook();

    let cancel = std::sync::Arc::new(AtomicBool::new(true));
    let (tx, rx) = std::sync::mpsc::channel();
    let config = AnalysisConfig {
        compute_log_mel: false,
        ..AnalysisConfig::default()
    };
    stagesync_desktop::jobs::spawn_analysis(
        tx,
        reference.clone(),
        vec![source.clone()],
        config,
        cancel,
    );

    // Collect the events until the worker reports how it ended.
    let mut cancelled = false;
    let mut finished = false;
    let mut failed = false;
    while let Ok(event) = rx.recv_timeout(std::time::Duration::from_secs(120)) {
        match event {
            stagesync_desktop::jobs::JobEvent::AnalysisCancelled => {
                cancelled = true;
                break;
            }
            stagesync_desktop::jobs::JobEvent::AnalysisFinished(_) => {
                finished = true;
                break;
            }
            stagesync_desktop::jobs::JobEvent::AnalysisFailed(_) => {
                failed = true;
                break;
            }
            _ => {}
        }
    }

    assert!(cancelled, "the cancelled run did not report a cancellation");
    assert!(!finished, "a cancelled run must not produce a report");
    assert!(!failed, "a cancelled run must not be reported as a failure");

    let _ = std::fs::remove_dir_all(&dir);
}
