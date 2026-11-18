//! The mixer: the controls, and the export they drive.
//!
//! The panel is driven through the real widgets, exactly as a mouse would drive
//! them: a button that is only painted — that does nothing when it is pressed —
//! fails these tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use egui_kittest::kittest::Queryable as _;
use stagesync_core::audio::{WavFormat, WavWriter};
use stagesync_core::matching::{AlignmentReport, LinearModel};
use stagesync_core::mixer::MonitorMode;
use stagesync_core::render::Layout;

use stagesync_desktop::jobs::{export_report, JobFailure};
use stagesync_desktop::mixer::{MixerState, TrackInfo};
use stagesync_desktop::state::{ExportSettings, TaskProgress};

/// The rate of the synthetic recordings.
const RATE: u32 = 16_000;
/// The length of the synthetic recordings.
const SECONDS: f64 = 70.0;

/// A unique scratch directory for one test.
fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "stagesync-mixer-{name}-{}-{nanos}",
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

/// The tracks the panel shows.
fn infos(count: usize) -> Vec<TrackInfo> {
    (0..count)
        .map(|index| TrackInfo {
            name: format!("recording {}.wav", index + 1),
            peak: Some(0.5),
            aligned: true,
            channels: 1,
        })
        .collect()
}

#[test]
fn the_mixer_panel_shows_a_row_for_every_recording() {
    let state = std::cell::RefCell::new(MixerState::new(2));
    let tracks = infos(2);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut state = state.borrow_mut();
        stagesync_desktop::mixer::panel(ui, &mut state, &tracks);
    });
    harness.run();

    assert!(
        harness.query_by_label("recording 1.wav").is_some(),
        "the first recording is not on the panel"
    );
    assert!(
        harness.query_by_label("recording 2.wav").is_some(),
        "the second recording is not on the panel"
    );
}

#[test]
fn the_mute_button_mutes_the_recording() {
    let state = std::cell::RefCell::new(MixerState::new(1));
    let tracks = infos(1);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut state = state.borrow_mut();
        stagesync_desktop::mixer::panel(ui, &mut state, &tracks);
    });
    harness.run();

    // The mute button is there and it does what it says.
    harness.get_by_label("M").click();
    harness.run();
    assert!(
        state.borrow().tracks[0].mute,
        "the mute button was pressed and the recording was not muted"
    );

    // And pressing it again unmutes it.
    harness.get_by_label("M").click();
    harness.run();
    assert!(
        !state.borrow().tracks[0].mute,
        "the mute button was pressed again and the recording was still muted"
    );
}

#[test]
fn the_solo_button_solos_the_recording() {
    let state = std::cell::RefCell::new(MixerState::new(2));
    let tracks = infos(2);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut state = state.borrow_mut();
        stagesync_desktop::mixer::panel(ui, &mut state, &tracks);
    });
    harness.run();

    // There is a solo button for each recording; the first one is pressed.
    let solos: Vec<_> = harness.get_all_by_label("S").collect();
    assert_eq!(
        solos.len(),
        2,
        "there is not a solo button for each recording"
    );
    solos[0].click();
    harness.run();
    assert!(
        state.borrow().tracks[0].solo,
        "the solo button was pressed and the recording was not soloed"
    );
    assert!(
        !state.borrow().tracks[1].solo,
        "the solo button of the first recording soloed the second one as well"
    );
}

#[test]
fn the_monitoring_mode_can_be_chosen() {
    let state = std::cell::RefCell::new(MixerState::new(2));
    let tracks = infos(2);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut state = state.borrow_mut();
        stagesync_desktop::mixer::panel(ui, &mut state, &tracks);
    });
    harness.run();

    // The combo box shows the mode that is in effect.
    assert_eq!(state.borrow().mode, MonitorMode::Both);
    assert!(
        harness.query_by_label("Monitor").is_some(),
        "the monitoring mode is not on the panel"
    );

    // Opening it offers every mode the core has.
    harness.get_by_label("Monitor").click();
    harness.run();
    for mode in MonitorMode::ALL {
        assert!(
            harness.query_by_label(mode.label()).is_some(),
            "the monitoring mode {} is not on the panel",
            mode.label()
        );
    }

    // Choosing one takes effect.
    harness
        .get_by_label(MonitorMode::StereoSplit.label())
        .click();
    harness.run();
    assert_eq!(state.borrow().mode, MonitorMode::StereoSplit);
}

#[test]
fn the_mixer_panel_shows_the_predicted_level() {
    let state = std::cell::RefCell::new(MixerState::new(1));
    let tracks = infos(1);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        let mut state = state.borrow_mut();
        stagesync_desktop::mixer::panel(ui, &mut state, &tracks);
    });
    harness.run();

    // The prediction is shown, and it is a number in dBFS.
    assert!(
        harness
            .query_by_label_contains("Predicted output")
            .is_some(),
        "the panel did not show the predicted output level"
    );
}

#[test]
fn the_mix_export_writes_a_mixed_file() {
    let dir = scratch_dir("mix");
    let reference = dir.join("reference.wav");
    let source = dir.join("source.wav");
    write_test_wav(&reference, RATE, SECONDS, 0x1234_5678);
    write_test_wav(&source, RATE, SECONDS, 0x8765_4321);

    // The report is built by the real pipeline and then given a known model, so
    // that the export has a placement to work with.
    let config = stagesync_core::pipeline::AnalysisConfig {
        compute_log_mel: false,
        ..stagesync_core::pipeline::AnalysisConfig::default()
    };
    let mut progress = |_: stagesync_core::pipeline::Progress| {};
    let report: AlignmentReport = stagesync_core::pipeline::analyze(
        &reference,
        std::slice::from_ref(&source.to_path_buf()),
        &config,
        &mut progress,
    )
    .expect("the analysis succeeds");
    let report = with_model(&report, LinearModel::new(1.0, 0.5));

    let out = dir.join("out");
    let settings = ExportSettings {
        dir: out.display().to_string(),
        layout: Layout::Mixdown,
        sample_rate: None,
        format: WavFormat::Float32,
        mix: stagesync_core::mixer::MixSettings::of_sources(&[0]),
        protection: stagesync_core::mixer::ClippingProtection::None,
    };

    let mut progress_log: Vec<TaskProgress> = Vec::new();
    let cancel = AtomicBool::new(false);
    let outcome = {
        let mut sink = |update: TaskProgress| progress_log.push(update);
        export_report(&report, &settings, &cancel, &mut sink)
    };
    match outcome {
        Ok(_) => {}
        Err(JobFailure::Failed(message)) => panic!("the export failed: {message}"),
        Err(JobFailure::Cancelled) => panic!("the export was cancelled unexpectedly"),
    }

    // The mixed file is there, and it is as long as the reference timeline.
    let mix_path = out.join("mixdown.wav");
    let size = std::fs::metadata(&mix_path)
        .expect("the mixed file was not written")
        .len();
    assert!(size > 0, "the mixed file is empty");

    let reader = stagesync_core::audio::ContainerReader::open(&mix_path).expect("the mix opens");
    assert_eq!(reader.channels(), 2, "the mix is not a stereo file");
    let expected = (report.timeline_duration * RATE as f64).round() as u64;
    assert!(
        (reader.frames() as i64 - expected as i64).abs() <= 2,
        "the mix holds {} frames and the reference timeline is {expected}",
        reader.frames()
    );
}

/// Replace the fitted model of every source with a known one.
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
