//! End-to-end `.stagesync` project persistence, relocation, staleness, recovery & export tests.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use stagesync_core::audio::{WavFormat, WavReader, WavWriter};
use stagesync_core::mixer::MonitorMode;
use stagesync_core::pipeline::{analyze, AnalysisConfig};
use stagesync_desktop::app::StageSyncApp;
use stagesync_desktop::jobs;
use stagesync_desktop::platform::CONFIG_DIR_ENV;
use stagesync_desktop::project::{self, MediaStatus, StageSyncProject};
use stagesync_desktop::state::TaskProgress;

static CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ConfigDirGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<String>,
}

impl ConfigDirGuard {
    fn set(dir: &std::path::Path) -> Self {
        let lock = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var(CONFIG_DIR_ENV).ok();
        std::env::set_var(CONFIG_DIR_ENV, dir);
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var(CONFIG_DIR_ENV, v),
            None => std::env::remove_var(CONFIG_DIR_ENV),
        }
    }
}

fn make_signal(secs: f64, rate: u32) -> Vec<f32> {
    let n = (secs * rate as f64).round() as usize;
    let mut state: u64 = 0x1234_5678_9ABC_DEF0;
    let mut rng = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        ((state.wrapping_mul(0x2545F491_4F6CDD1D) >> 40) as f64) / (1u64 << 24) as f64
    };
    let mut out = vec![0.0f32; n];
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f64 / rate as f64;
        let mod_env = 0.25 + 0.75 * (2.0 * std::f64::consts::PI * 0.27 * t).sin().abs();
        let tone = (2.0 * std::f64::consts::PI * (190.0 + 40.0 * (0.4 * t).sin()) * t).sin()
            + 0.4 * (2.0 * std::f64::consts::PI * 417.0 * t).sin();
        *s = (tone * mod_env * 0.3 + 0.002 * (rng() * 2.0 - 1.0)) as f32;
    }
    out
}

fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) {
    let mut w = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create wav");
    w.write_planar(std::slice::from_ref(&samples))
        .expect("write wav");
    w.finalize().expect("finalize wav");
}

#[test]
fn project_round_trips_state_detects_modifications_relocates_and_exports() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let audio_dir = temp.path().join("recordings");
    fs::create_dir_all(&audio_dir).unwrap();
    let ref_path = audio_dir.join("ref.wav");
    let tgt_path = audio_dir.join("mic.wav");

    let rate = 16_000u32;
    let signal = make_signal(42.0, rate);
    write_wav(&ref_path, &signal, rate);
    // Target starts 1.5s into reference.
    let offset_samples = (1.5 * rate as f64) as usize;
    write_wav(&tgt_path, &signal[offset_samples..], rate);

    let config = AnalysisConfig {
        compute_log_mel: false,
        measure_waveform_drift: false,
        ..AnalysisConfig::default()
    };
    let mut noop = |_: stagesync_core::pipeline::Progress| {};
    let report = analyze(
        &ref_path,
        std::slice::from_ref(&tgt_path),
        &config,
        &mut noop,
    )
    .expect("analyze succeeds");
    assert!(report.sources[0].model().is_some());

    // Populate StageSyncApp with analyzed report, custom mixer, nudge, anchor, timeline, export.
    let mut app = StageSyncApp::new();
    app.open_files(vec![ref_path.clone(), tgt_path.clone()]);

    let proj_path = temp.path().join("concert.stagesync");
    let mut proj = app.to_project(Some(&proj_path));
    drop(app);
    proj.report = Some(report);
    proj.targets[0].pending_nudge_secs = 0.012;
    proj.targets[0].anchor.source_time = 2.0;
    proj.targets[0].anchor.reference_time = 3.5;
    proj.targets[0].anchor.active = false;
    proj.mixer.mode = MonitorMode::StereoSplit;
    proj.mixer.master_gain = 0.75;
    proj.mixer.tracks[0].gain = 0.5;
    proj.mixer.tracks[0].trim = 1.25;
    proj.mixer.tracks[0].pan = -0.35;
    proj.timeline.playhead = 7.5;
    proj.timeline.view_start = 2.0;
    proj.timeline.view_end = 22.0;
    proj.timeline.loop_region = Some((4.0, 12.0));
    proj.export.dir = temp.path().join("exports").display().to_string();
    proj.export.layout = "mixdown".to_string();
    proj.save_to(&proj_path).expect("save project");

    // Verify zero huge audio bytes embedded (JSON size stays tiny compared to >2 MB WAVs).
    let proj_bytes = fs::metadata(&proj_path).unwrap().len();
    let audio_bytes = fs::metadata(&ref_path).unwrap().len();
    assert!(
        proj_bytes < audio_bytes / 10,
        "project ({proj_bytes} B) should not embed audio ({audio_bytes} B)"
    );

    // Reopen into a fresh app, verify state & export directly.
    let mut reopened = StageSyncApp::new();
    reopened
        .open_project_path(&proj_path)
        .expect("open saved project");
    assert!(!reopened.is_dirty());
    assert_eq!(reopened.project_path(), Some(proj_path.as_path()));
    drop(reopened);

    let loaded_raw = StageSyncProject::load_from(&proj_path).unwrap();
    assert!(loaded_raw.valid_report.is_some());
    assert_eq!(
        loaded_raw.reference.as_ref().unwrap().status,
        MediaStatus::Verified
    );

    // Export mixdown directly from the loaded report without re-analyzing.
    let report_to_export = loaded_raw.valid_report.unwrap();
    let export_settings =
        loaded_raw
            .project
            .export
            .to_export_settings(&stagesync_desktop::mixer::MixerState {
                tracks: loaded_raw.project.mixer.tracks.clone(),
                mode: loaded_raw.project.mixer.mode,
                master_gain: loaded_raw.project.mixer.master_gain,
                ..Default::default()
            });
    let mut prog = Vec::<TaskProgress>::new();
    let cancel = AtomicBool::new(false);
    let outcome = jobs::export_report(&report_to_export, &export_settings, &cancel, &mut |p| {
        prog.push(p)
    })
    .expect("export from reopened project succeeds");
    let mixdown = out_file(&outcome.files);
    let reader = WavReader::open(&mixdown).expect("read exported mixdown");
    assert!(reader.frames() > 0);

    // Modify target audio on disk -> cached alignment report MUST be invalidated!
    let mut tampered = fs::read(&tgt_path).unwrap();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    fs::write(&tgt_path, &tampered).unwrap();

    let stale = StageSyncProject::load_from(&proj_path).expect("load modified");
    assert!(
        stale.valid_report.is_none(),
        "stale cached report must never be reused when a recording was modified"
    );
    assert!(matches!(
        stale.targets[0].status,
        MediaStatus::ModifiedOnDisk { .. }
    ));

    // Delete/move target audio -> Missing + Relocate restores it!
    let relocated_path = temp.path().join("moved_mic.wav");
    fs::rename(&tgt_path, &relocated_path).unwrap();
    let mut missing_app = StageSyncApp::new();
    missing_app
        .open_project_path(&proj_path)
        .expect("open with missing file");
    missing_app.relocate_target(0, relocated_path);
    assert!(missing_app.is_dirty());
    drop(missing_app);
}

fn out_file(files: &[PathBuf]) -> PathBuf {
    files
        .iter()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")))
        .cloned()
        .expect("at least one exported WAV file")
}

#[test]
fn crash_recovery_snapshot_saves_and_clears() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let wav = temp.path().join("song.wav");
    write_wav(&wav, &make_signal(2.0, 8000), 8000);

    let mut app = StageSyncApp::new();
    app.open_files(vec![wav.clone()]);
    let snap = app.to_project(None);
    project::write_recovery(&snap, None, true).expect("write recovery");

    let meta = project::load_recovery_meta().expect("recovery meta exists");
    assert!(meta.dirty);

    let (loaded, _) = project::load_recovery_project().expect("load recovery");
    assert_eq!(loaded.reference.unwrap().resolved_path, wav);

    project::clear_recovery();
    assert!(project::load_recovery_meta().is_none());
    let _ = Duration::from_secs(0);
}
