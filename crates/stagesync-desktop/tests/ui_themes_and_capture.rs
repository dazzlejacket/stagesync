//! Populated GUI workflow, Light/Dark theme switching, responsiveness, and screenshot capture tests.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use stagesync_core::audio::{WavFormat, WavWriter};
use stagesync_desktop::app::StageSyncApp;
use stagesync_desktop::platform::CONFIG_DIR_ENV;
use stagesync_desktop::settings::AppSettings;
use stagesync_desktop::theme::ThemeMode;

struct ConfigGuard {
    prev: Option<String>,
}

impl ConfigGuard {
    fn set(dir: &std::path::Path) -> Self {
        let prev = std::env::var(CONFIG_DIR_ENV).ok();
        std::env::set_var(CONFIG_DIR_ENV, dir);
        Self { prev }
    }
}

impl Drop for ConfigGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(CONFIG_DIR_ENV, v),
            None => std::env::remove_var(CONFIG_DIR_ENV),
        }
    }
}

fn stereo_concert_signal(secs: f64, rate: u32, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let n = (secs * rate as f64).round() as usize;
    let mut state = seed | 1;
    let mut rng = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        ((state.wrapping_mul(0x2545F491_4F6CDD1D) >> 40) as f64) / (1u64 << 24) as f64
    };
    let mut l = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];

    // Divide into unique pseudo-random musical phrases so there is no periodicity
    // across the recording.
    let mut idx = 0usize;
    let mut phase1 = 0.0f64;
    let mut phase2 = 0.0f64;
    let mut phase3 = 0.0f64;
    while idx < n {
        let dur_samples = ((0.18 + rng() * 0.42) * rate as f64).round() as usize;
        let end = (idx + dur_samples).min(n);
        let f1 = 110.0 + rng() * 320.0;
        let f2 = f1 * (1.29 + rng() * 0.43);
        let f3 = f1 * (1.87 + rng() * 0.55);
        let level = if rng() < 0.10 {
            0.12
        } else {
            0.30 + rng() * 0.65
        };
        let pan = rng() * 0.4 + 0.3;
        let attack = (0.015 * rate as f64).max(1.0);
        for (k, pos) in (idx..end).enumerate() {
            let env = ((k as f64) / attack).min(1.0)
                * (1.0 - (k as f64) / (dur_samples as f64 * 1.15)).max(0.2);
            phase1 += 2.0 * std::f64::consts::PI * f1 / rate as f64;
            phase2 += 2.0 * std::f64::consts::PI * f2 / rate as f64;
            phase3 += 2.0 * std::f64::consts::PI * f3 / rate as f64;
            let body =
                (phase1.sin() * 0.48 + phase2.sin() * 0.32 + phase3.sin() * 0.20) * level * env;
            let transient = if k < (0.025 * rate as f64) as usize {
                0.28 * level * (rng() * 2.0 - 1.0)
            } else {
                0.0
            };
            let hiss = 0.002 * (rng() * 2.0 - 1.0);
            l[pos] = ((body * pan + transient + hiss) * 0.72) as f32;
            r[pos] = ((body * (1.0 - pan * 0.5) + transient * 0.85 + hiss) * 0.72) as f32;
        }
        idx = end;
    }
    (l, r)
}

fn slice_with_drift(samples: &[f32], offset_secs: f64, scale: f64, rate: u32) -> Vec<f32> {
    // Convention: reference_time = scale * source_time + offset_secs
    let max_source_secs = ((samples.len() as f64 / rate as f64) - offset_secs - 0.1) / scale;
    let out_len = (max_source_secs.max(0.0) * rate as f64).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_t = i as f64 / rate as f64;
        let ref_t = scale * src_t + offset_secs;
        let f_idx = ref_t * rate as f64;
        let lo = f_idx.floor() as usize;
        let frac = (f_idx - lo as f64) as f32;
        let s = if lo + 1 < samples.len() {
            samples[lo] * (1.0 - frac) + samples[lo + 1] * frac
        } else {
            samples[lo]
        };
        out.push(s);
    }
    out
}

fn write_wav_stereo(path: &std::path::Path, l: &[f32], r: &[f32], rate: u32) {
    let mut w = WavWriter::create(path, rate, 2, WavFormat::Float32).expect("create stereo wav");
    w.write_planar(&[l, r]).expect("write stereo wav");
    w.finalize().expect("finalize stereo wav");
}

fn write_wav_mono(path: &std::path::Path, samples: &[f32], rate: u32) {
    let mut w = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create mono wav");
    w.write_planar(&[samples]).expect("write mono wav");
    w.finalize().expect("finalize mono wav");
}

#[test]
fn populated_ui_supports_dark_light_tabs_and_responsive_sizes() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let _cfg = ConfigGuard::set(temp.path());
    let rate = 16_000u32;
    let (ref_l, ref_r) = stereo_concert_signal(120.0, rate, 0xCAFE01);
    let ref_path = temp.path().join("BoardMix_Stereo.wav");
    write_wav_stereo(&ref_path, &ref_l, &ref_r, rate);

    // Target 1: stereo handheld delayed by 2.4 s with +45 ppm clock drift.
    let tgt1_l = slice_with_drift(&ref_l, 2.4, 1.000_045, rate);
    let tgt1_r = slice_with_drift(&ref_r, 2.4, 1.000_045, rate);
    let tgt1_path = temp.path().join("ZoomH5_StageLeft.wav");
    write_wav_stereo(&tgt1_path, &tgt1_l, &tgt1_r, rate);

    // Target 2: mono audience phone delayed by 5.1 s and quieter.
    let mono: Vec<f32> = slice_with_drift(&ref_l, 5.1, 1.0, rate)
        .into_iter()
        .map(|s| s * 0.45)
        .collect();
    let tgt2_path = temp.path().join("Audience_Phone.wav");
    write_wav_mono(&tgt2_path, &mono, rate);

    let capture_dir = std::env::var_os("STAGESYNC_SCREENSHOT_DIR").map(|raw| {
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|p| p.parent())
                .map(|root| root.join(&p))
                .unwrap_or(p)
        }
    });
    let builder = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1400.0, 860.0))
        .with_max_steps(10_000);
    let builder = if capture_dir.is_some() {
        builder.wgpu()
    } else {
        builder
    };

    let mut harness = builder.build_eframe(|_cc| StageSyncApp::default());
    harness
        .state_mut()
        .open_files(vec![ref_path.clone(), tgt1_path.clone(), tgt2_path.clone()]);
    harness.run();

    harness
        .query_by_label("Analyze")
        .expect("Analyze button exists")
        .click();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !(harness.state().has_report() && harness.state().has_all_peaks())
        && std::time::Instant::now() < deadline
    {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(harness.state().has_report(), "analysis did not finish");
    harness.state_mut().select_target(0);
    harness.run();

    // Verify populated Fit & Mixer output.
    harness.query_by_label("Fit").expect("Fit button").click();
    harness.run();
    assert!(harness
        .query_by_label_contains("Predicted output")
        .is_some());

    if let Some(dir) = &capture_dir {
        std::fs::create_dir_all(dir).expect("create screenshot dir");
        harness.state_mut().set_theme_mode(ThemeMode::Dark);
        harness.run();
        let dark_img = harness.render().expect("render dark mixer screenshot");
        dark_img
            .save(dir.join("stagesync_dark_mixer.png"))
            .expect("save dark png");

        harness.state_mut().set_theme_mode(ThemeMode::Light);
        harness.run();
        let light_img = harness.render().expect("render light mixer screenshot");
        light_img
            .save(dir.join("stagesync_light_mixer.png"))
            .expect("save light png");

        harness
            .query_by_label("Inspector & Charts")
            .expect("Inspector tab")
            .click();
        harness.state_mut().set_theme_mode(ThemeMode::Dark);
        harness.run();
        let insp_img = harness.render().expect("render inspector screenshot");
        insp_img
            .save(dir.join("stagesync_dark_inspector.png"))
            .expect("save inspector png");
    } else {
        // Headless theme cycling + tab switching assertions (works on CI without GPU).
        harness.state_mut().set_theme_mode(ThemeMode::Light);
        harness.run();
        harness
            .query_by_label("Inspector & Charts")
            .expect("Inspector tab")
            .click();
        harness.run();
        assert!(harness.query_by_label_contains("Drift verdict").is_some());
        harness
            .query_by_label_contains("Issues & Manual Anchor")
            .expect("Issues tab")
            .click();
        harness.run();
        assert!(harness.query_by_label("Manual anchor").is_some());
        harness.state_mut().set_theme_mode(ThemeMode::Dark);
        harness.run();
    }

    // Verify compact 960x640 window size stays usable without layout panics or overlapping transport labels.
    harness.set_size(egui::vec2(960.0, 640.0));
    harness.run();
    assert!(harness.query_by_label("Analyze").is_some());
    let tc_rect = harness
        .query_by_label_contains("Playhead ")
        .expect("timecode readout present at 960x640")
        .rect();
    let fit_rect = harness
        .query_by_label("Fit")
        .expect("Fit button present at 960x640")
        .rect();
    let span_rect = harness
        .query_by_label_contains("s / ")
        .expect("view span label present at 960x640")
        .rect();
    assert!(
        !tc_rect.intersects(span_rect) && !tc_rect.intersects(fit_rect),
        "compact 960x640 transport labels collided: timecode={tc_rect:?}, span={span_rect:?}, fit={fit_rect:?}"
    );
    assert!(
        span_rect.right() <= 960.0 && tc_rect.right() <= 960.0,
        "compact 960x640 transport overflowed window width: tc={tc_rect:?}, span={span_rect:?}"
    );

    // Also verify default window never leaves an empty/default-setting panic.
    let _ = AppSettings::default();
}
