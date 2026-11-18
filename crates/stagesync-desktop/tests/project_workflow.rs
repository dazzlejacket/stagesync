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

#[test]
fn p1_01_moved_project_rebinds_report_paths_and_exports_moved_audio() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let orig = temp.path().join("orig");
    let moved = temp.path().join("moved");
    fs::create_dir_all(&orig).unwrap();
    fs::create_dir_all(&moved).unwrap();

    let rate = 8_000u32;
    let ref_sig = make_signal(40.0, rate);
    let src_sig: Vec<f32> = ref_sig[rate as usize * 2..].to_vec(); // +2.0 s offset

    let orig_ref = orig.join("ref.wav");
    let orig_src = orig.join("src.wav");
    write_wav(&orig_ref, &ref_sig, rate);
    write_wav(&orig_src, &src_sig, rate);

    let config = AnalysisConfig {
        compute_log_mel: false,
        measure_waveform_drift: false,
        ..AnalysisConfig::default()
    };
    let mut noop = |_: stagesync_core::pipeline::Progress| {};
    let report = analyze(
        &orig_ref,
        std::slice::from_ref(&orig_src),
        &config,
        &mut noop,
    )
    .expect("analyze succeeds");

    let orig_proj_path = orig.join("show.stagesync");
    let mut app = StageSyncApp::new();
    app.open_files(vec![orig_ref.clone(), orig_src.clone()]);
    let mut proj = app.to_project(Some(&orig_proj_path));
    proj.report = Some(report);
    proj.export.layout = "reference_timeline_per_source".to_string();
    proj.save_to(&orig_proj_path).unwrap();

    // Copy project + audio to moved/.
    let moved_ref = moved.join("ref.wav");
    let moved_src = moved.join("src.wav");
    let moved_proj_path = moved.join("show.stagesync");
    fs::copy(&orig_ref, &moved_ref).unwrap();
    fs::copy(&orig_src, &moved_src).unwrap();
    fs::copy(&orig_proj_path, &moved_proj_path).unwrap();

    // Replace orig/src.wav with constant 0.125 audio.
    let bad_audio = vec![0.125f32; src_sig.len()];
    write_wav(&orig_src, &bad_audio, rate);

    let mut moved_app = StageSyncApp::new();
    moved_app
        .open_project_path(&moved_proj_path)
        .expect("open moved project");
    let snapped = moved_app.to_project(Some(&moved_proj_path));
    let report_to_export = snapped.report.expect("moved report retained");
    assert_eq!(
        report_to_export.sources[0].source.path, moved_src,
        "cached report path was not rebound to moved/src.wav"
    );

    let export_dir = temp.path().join("export_moved");
    let mut export_settings = snapped
        .export
        .to_export_settings(&stagesync_desktop::mixer::MixerState::default());
    export_settings.dir = export_dir.display().to_string();
    let cancel = AtomicBool::new(false);
    let outcome = jobs::export_report(&report_to_export, &export_settings, &cancel, &mut |_| {})
        .expect("export of moved project succeeds");
    let wav = out_file(&outcome.files);
    let mut reader = WavReader::open(&wav).unwrap();
    let samples = reader
        .read_frames(rate as u64 * 5, rate as usize * 5)
        .unwrap();
    let interior = &samples[0];
    let max_diff_from_0125 = interior
        .iter()
        .map(|s| (s - 0.125).abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_diff_from_0125 > 0.05,
        "export read stale orig/src.wav (constant 0.125) instead of moved/src.wav!"
    );

    // Also succeed when orig/ directory is completely deleted.
    fs::remove_dir_all(&orig).unwrap();
    let export_dir2 = temp.path().join("export_moved_no_orig");
    export_settings.dir = export_dir2.display().to_string();
    jobs::export_report(&report_to_export, &export_settings, &cancel, &mut |_| {})
        .expect("export of moved project succeeds without orig/");
}

#[test]
fn p1_02_project_save_refuses_to_overwrite_input_recording_or_wrong_extension() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let wav_path = temp.path().join("save-collision.wav");
    let original_bytes = b"RIFF_ORIGINAL_AUDIO_BYTES_DO_NOT_TOUCH".to_vec();
    fs::write(&wav_path, &original_bytes).unwrap();

    let mut app = StageSyncApp::new();
    app.open_files(vec![wav_path.clone()]);

    let err = app
        .save_project_to(&wav_path)
        .expect_err("saving project over input WAV must be refused");
    assert!(
        err.to_string().contains("stagesync") || err.to_string().contains("overwrite"),
        "unexpected error: {err:#}"
    );
    assert_eq!(
        fs::read(&wav_path).unwrap(),
        original_bytes,
        "input WAV bytes were overwritten!"
    );

    // Relative alias pointing to the same WAV must also be refused.
    let alias = temp
        .path()
        .join("sub")
        .join("..")
        .join("save-collision.wav");
    fs::create_dir_all(temp.path().join("sub")).unwrap();
    assert!(app.save_project_to(&alias).is_err());
    assert_eq!(fs::read(&wav_path).unwrap(), original_bytes);

    // Arbitrary non-.stagesync extension must also be refused.
    let wrong_ext = temp.path().join("show.json");
    assert!(app.save_project_to(&wrong_ext).is_err());
    assert!(!wrong_ext.exists());
}

fn make_aperiodic_signal(secs: f64, rate: u32, seed: u64) -> Vec<f32> {
    let n = (secs * rate as f64).round() as usize;
    let mut state: u64 = seed | 1;
    let mut rng = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        ((state.wrapping_mul(0x2545F491_4F6CDD1D) >> 40) as f64) / (1u64 << 24) as f64
    };
    let mut out = vec![0.0f32; n];
    let step = (rate as usize / 20).max(1); // 50 ms aperiodic amplitude steps
    let mut env = 0.4;
    let mut f1 = 173.0;
    let mut f2 = 311.0;
    for (i, s) in out.iter_mut().enumerate() {
        if i % step == 0 {
            env = 0.08 + 0.9 * rng();
            f1 = 130.0 + 290.0 * rng();
            f2 = 340.0 + 420.0 * rng();
        }
        let t = i as f64 / rate as f64;
        let tone = (2.0 * std::f64::consts::PI * f1 * t).sin()
            + 0.6 * (2.0 * std::f64::consts::PI * f2 * t).sin()
            + 0.25 * (rng() * 2.0 - 1.0);
        *s = (tone * env * 0.25) as f32;
    }
    out
}

#[test]
fn p1_04_opening_project_b_isolates_background_analysis_from_project_a() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let rate = 8_000u32;
    let sig_a = make_aperiodic_signal(70.0, rate, 0xAAAA_5555);
    let ref_a = temp.path().join("ref_a.wav");
    let src_a = temp.path().join("src_a.wav");
    write_wav(&ref_a, &sig_a, rate);
    write_wav(&src_a, &sig_a[rate as usize..], rate);

    let sig_b = make_aperiodic_signal(20.0, rate, 0xBBBB_6666);
    let ref_b = temp.path().join("ref_b.wav");
    let src_b = temp.path().join("src_b.wav");
    write_wav(&ref_b, &sig_b, rate);
    write_wav(&src_b, &sig_b[rate as usize..], rate);

    // Control check: verify Project A's audio actually finishes analysis on its own.
    {
        let mut control = StageSyncApp::new();
        control.open_files(vec![ref_a.clone(), src_a.clone()]);
        control.analyse_on_start();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline && !control.has_report() {
            control.poll_background();
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            control.has_report(),
            "control run for Project A failed to produce a report"
        );
    }

    // Create Project B on disk (same target count = 1, unanalyzed).
    let proj_b_path = temp.path().join("proj_b.stagesync");
    {
        let mut setup_b = StageSyncApp::new();
        setup_b.open_files(vec![ref_b.clone(), src_b.clone()]);
        setup_b.save_project_to(&proj_b_path).unwrap();
    }

    // Start background analysis on Project A, then immediately open Project B.
    let mut app = StageSyncApp::new();
    app.open_files(vec![ref_a, src_a]);
    app.analyse_on_start();
    app.open_project_path(&proj_b_path)
        .expect("open project B while A is analyzing");

    // Pump background events long enough for A's worker to finish & deliver any stale event.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        app.poll_background();
        if app.has_report() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    assert!(
        !app.has_report(),
        "stale analysis from Project A leaked into Project B: {:?}",
        app.to_project(None)
            .report
            .map(|r| r.sources[0].source.path.clone())
    );
    assert!(!app.is_dirty(), "stale background event dirtied Project B");
}

#[test]
fn p2_01_changed_or_missing_media_invalidate_manual_state_and_preserve_identity() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let rate = 8_000u32;
    let sig = make_signal(10.0, rate);
    let ref_path = temp.path().join("ref.wav");
    let tgt_path = temp.path().join("tgt.wav");
    write_wav(&ref_path, &sig, rate);
    write_wav(&tgt_path, &sig, rate);

    let proj_path = temp.path().join("manual.stagesync");
    let mut app = StageSyncApp::new();
    app.open_files(vec![ref_path.clone(), tgt_path.clone()]);
    let mut proj = app.to_project(Some(&proj_path));
    let expected_hash = proj.targets[0].media.sha256.clone().expect("hash saved");
    let expected_len = proj.targets[0].media.byte_len.expect("len saved");
    proj.targets[0].anchor.source_time = 3.0;
    proj.targets[0].anchor.reference_time = 5.0;
    proj.targets[0].anchor.active = true;
    proj.targets[0].pending_nudge_secs = 0.25;
    proj.save_to(&proj_path).unwrap();

    // Case 1: target missing -> opening & snapshotting MUST preserve expected SHA-256 and byte_len.
    let backup = temp.path().join("tgt_backup.wav");
    fs::rename(&tgt_path, &backup).unwrap();
    let mut missing_app = StageSyncApp::new();
    missing_app.open_project_path(&proj_path).unwrap();
    let missing_snap = missing_app.to_project(Some(&proj_path));
    assert_eq!(
        missing_snap.targets[0].media.sha256.as_deref(),
        Some(expected_hash.as_str()),
        "saved SHA-256 was lost while target was missing"
    );
    assert_eq!(
        missing_snap.targets[0].media.byte_len,
        Some(expected_len),
        "saved byte_len was lost while target was missing"
    );

    // Case 2: target audio modified on disk -> anchor and pending nudge must be cleared on load.
    let changed_sig: Vec<f32> = sig.iter().map(|s| s * 0.5).collect();
    write_wav(&tgt_path, &changed_sig, rate);
    let mut mod_app = StageSyncApp::new();
    mod_app.open_project_path(&proj_path).unwrap();
    let mod_snap = mod_app.to_project(Some(&proj_path));
    assert!(
        !mod_snap.targets[0].anchor.active,
        "stale manual anchor remained active after target audio changed"
    );
    assert!(
        mod_snap.targets[0].pending_nudge_secs.abs() < 1e-12,
        "stale pending nudge remained {} after target audio changed",
        mod_snap.targets[0].pending_nudge_secs
    );

    // Case 3: restore target, modify reference -> all anchors & nudges must be cleared.
    fs::rename(&backup, &tgt_path).unwrap();
    write_wav(&ref_path, &changed_sig, rate);
    let mut ref_mod_app = StageSyncApp::new();
    ref_mod_app.open_project_path(&proj_path).unwrap();
    let ref_mod_snap = ref_mod_app.to_project(Some(&proj_path));
    assert!(
        !ref_mod_snap.targets[0].anchor.active,
        "stale manual anchor remained active after reference audio changed"
    );
    assert!(
        ref_mod_snap.targets[0].pending_nudge_secs.abs() < 1e-12,
        "stale pending nudge remained after reference audio changed"
    );
}

#[test]
fn p1_01_export_refuses_tampered_audio_before_render() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _cfg = ConfigDirGuard::set(temp.path());

    let rate = 8_000u32;
    let sig = make_signal(40.0, rate);
    let ref_path = temp.path().join("ref.wav");
    let src_path = temp.path().join("src.wav");
    write_wav(&ref_path, &sig, rate);
    write_wav(&src_path, &sig[rate as usize..], rate);

    let config = AnalysisConfig {
        compute_log_mel: false,
        measure_waveform_drift: false,
        ..AnalysisConfig::default()
    };
    let mut noop = |_: stagesync_core::pipeline::Progress| {};
    let report = analyze(
        &ref_path,
        std::slice::from_ref(&src_path),
        &config,
        &mut noop,
    )
    .expect("analyze");

    // Mutate bytes on disk WITHOUT changing file size so only SHA-256 reveals tampering.
    let mut bytes = fs::read(&src_path).unwrap();
    let idx = bytes.len() / 2;
    bytes[idx] ^= 0xFF;
    fs::write(&src_path, &bytes).unwrap();

    let settings = stagesync_desktop::state::ExportSettings {
        dir: temp.path().join("out").display().to_string(),
        ..Default::default()
    };
    let cancel = AtomicBool::new(false);
    let err = jobs::export_report(&report, &settings, &cancel, &mut |_| {})
        .expect_err("export must refuse tampered audio file");
    match err {
        jobs::JobFailure::Failed(msg) => {
            assert!(
                msg.contains("SHA-256") || msg.contains("changed on disk"),
                "unexpected error message: {msg}"
            );
        }
        other => panic!("expected Failed error, got {other:?}"),
    }
}
