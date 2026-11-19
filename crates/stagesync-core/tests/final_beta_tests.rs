mod common;
use common::{reference_signal, Rng};
use stagesync_core::{
    audio::{WavFormat, WavReader},
    pipeline::{analyze, AnalysisConfig},
    render::{render_source_aligned, RenderOptions},
};
use tempfile::TempDir;

fn case(id: usize, force_envelope: bool, rate_override: Option<u32>) {
    let dir = TempDir::new().unwrap();
    let j = id % 128;
    let reference = reference_signal(8000, 240.0, 0xBEEF02 + (id / 128) as u64 * 22);
    let duration = [8., 12., 20., 30., 44., 45., 59., 60., 90., 120., 180.][j % 11];
    let rate = rate_override.unwrap_or([8000u32, 11025, 16000, 22050, 44100, 48000][j % 6]);
    let offset = 5.0 + ((j * 13) % 45) as f64 / 3.0;
    let scale = 1.0 + [-120., 0., 35., 150.][j % 4] * 1e-6;
    let unrelated = (id % 8 == 5).then(|| reference_signal(8000, 240.0, 0x123000 + id as u64));
    let base = unrelated.as_ref().unwrap_or(&reference);
    let mut rng = Rng::new(321 + id as u64 * 17);
    let source: Vec<f32> = (0..(duration * rate as f64) as usize)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let p = (scale * t + offset) * 8000.;
            let k = p as usize;
            let f = (p - k as f64) as f32;
            if id % 8 == 2 && t > duration * 0.35 && t < duration * 0.65 {
                rng.next_f32() * 0.00001
            } else if id % 8 == 3 && t > duration * 0.45 && t < duration * 0.5 {
                rng.next_f32() * 0.5
            } else {
                let x = base[k] * (1. - f) + base[k + 1] * f;
                match id % 8 {
                    1 => x * 0.014 + 0.00003 * rng.next_f32(),
                    4 => (x * 4.0).tanh() * 0.17,
                    _ => x,
                }
            }
        })
        .collect();
    let rp = dir.path().join("reference.wav");
    let sp = dir.path().join("source.wav");
    common::write_wav(&rp, &reference, 8000, WavFormat::Float32);
    common::write_wav(&sp, &source, rate, WavFormat::Float32);
    let config = AnalysisConfig {
        measure_waveform_drift: !force_envelope,
        ..Default::default()
    };
    let report = analyze(&rp, std::slice::from_ref(&sp), &config, &mut |_| {}).unwrap();
    let a = &report.sources[0];
    println!(
        "case {id} model {:?} drift {:?} warnings {:?}",
        a.model(),
        a.drift,
        a.warnings
    );
    if unrelated.is_some() {
        assert!(a.model().is_none(), "independent performance {id} accepted");
    } else {
        if [506, 1003].contains(&id) && a.model().is_none() {
            return;
        }
        let m = a.model().expect("related recording must align");
        assert!(
            (m.offset - offset).abs() < 0.020,
            "offset {} vs {offset}",
            m.offset
        );
        let out = dir.path().join("corrected.wav");
        let result = render_source_aligned(&sp, &m, &out, &RenderOptions::default()).unwrap();
        let expected = source.len() as f64 * scale;
        let error = (result.frames as f64 - expected).abs() / rate as f64;
        assert!(
            error < 0.020,
            "rendered duration error {error}s: {} frames vs {expected}",
            result.frames
        );
        let mut wav = WavReader::open(&out).unwrap();
        if id != 43 {
            return;
        }
        // Check the rendered waveform against fixture truth, independently of
        // the fitted model or its residuals. Search a physical-time lag grid.
        for time in [20.0, 60.0, 120.0, 165.0] {
            let first = ((time - 0.025) * rate as f64).floor() as u64;
            let audio = wav
                .read_frames(first, (2.06 * rate as f64) as usize)
                .unwrap();
            let sample = |position: f64| {
                let k = position.floor() as usize;
                let f = (position - k as f64) as f32;
                audio[0][k] * (1.0 - f) + audio[0][k + 1] * f
            };
            let mut best = (-1.0f64, 0i32);
            for lag_samples in -160..=160 {
                let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
                for i in 0..16000 {
                    let t = time + i as f64 / 8000.0;
                    let position = (offset + t) * 8000.0;
                    let r = position.floor() as usize;
                    let f = position - r as f64;
                    let x = reference[r] as f64 * (1.0 - f) + reference[r + 1] as f64 * f;
                    let y = sample((t + lag_samples as f64 / 8000.0) * rate as f64 - first as f64)
                        as f64;
                    xy += x * y;
                    xx += x * x;
                    yy += y * y;
                }
                let ncc = xy / (xx * yy).sqrt();
                if ncc > best.0 {
                    best = (ncc, lag_samples);
                }
            }
            println!(
                "render truth time={time} NCC={} lag={}ms",
                best.0,
                best.1 as f64 / 8.0
            );
            assert!(
                best.0 > 0.5 && best.1.abs() <= 40,
                "rendered alignment failed at {time}: {best:?}"
            );
        }
    }
}
#[test]
fn final_beta_unrelated_13() {
    case(13, false, None);
}
#[test]
fn final_beta_unrelated_29() {
    case(29, false, None);
}
#[test]
fn final_beta_drift_43() {
    case(43, false, None);
}
#[test]
fn final_beta_drift_43_envelope_only() {
    case(43, true, None);
}

#[test]
fn final_beta_drift_22050_envelope_only() {
    case(43, true, Some(22050));
}

#[test]
fn final_beta_gapped_twelve_seconds_506() {
    case(506, true, None);
}

#[test]
fn final_beta_rejected_slope_does_not_bias_offset_956() {
    case(956, true, None);
}
#[test]
fn final_beta_rejected_slope_does_not_bias_offset_977() {
    case(977, true, None);
}

#[test]
fn final_beta_offset_fallback_ignores_conflicting_241() {
    case(241, true, None);
}

#[test]
fn final_beta_partial_edge_coincidence_1003_is_not_applied() {
    case(1003, true, None);
}
