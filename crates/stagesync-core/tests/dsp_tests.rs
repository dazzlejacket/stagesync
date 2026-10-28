//! Unit tests for the DSP primitives.
//!
//! These are the tests that pin down the *meaning* of the library's conventions:
//! which way a lag points, how a time map is applied, and what a resampler does to
//! amplitude and phase.

use stagesync_core::features::envelope::{local_zscore, percentile};
use stagesync_core::matching::dsp::{find_peaks, gcc_phat, ncc_correlate, Correlation};
use stagesync_core::matching::model::{LinearModel, MapSegment, TimeMap};
use stagesync_core::matching::regression::{least_squares, ransac, robust_fit, theil_sen};
use stagesync_core::resample::{resample_buffer, PolyphaseFilter, StreamingResampler};

/// A deterministic pseudo-random signal.
fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut state = seed | 1;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 32) as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

/// A broadband tonal mixture.
///
/// The partials are deliberately *not* harmonically related and a little noise is added,
/// because a handful of harmonically related pure tones share a common period: with
/// such a signal two completely different positions in a recording are genuinely
/// indistinguishable, which is a property of the test signal rather than of the
/// matcher under test.
fn tones(n: usize, rate: f64) -> Vec<f32> {
    let mut state = 0x9e37_79b9_u64 | 1;
    let mut noise = Vec::with_capacity(n);
    for _ in 0..n {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise.push(((state >> 32) as f32 / u32::MAX as f32) * 2.0 - 1.0);
    }
    let partials = [220.0, 331.0, 443.0, 557.0, 669.0];
    (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            let mut v = 0.0f64;
            for (k, f) in partials.iter().enumerate() {
                v += 0.5 / (k as f64 + 1.0) * (2.0 * std::f64::consts::PI * f * t).sin();
            }
            (v + 0.05 * noise[i] as f64) as f32
        })
        .collect()
}

#[test]
fn ncc_peaks_at_the_true_lag() {
    let reference = tones(20_000, 48_000.0);
    let shift = 137usize; // the query starts this many samples into the reference
    let query = &reference[shift..shift + 4_000];

    let corr = ncc_correlate(query, &reference);
    let peaks = find_peaks(&corr, 3, 10);
    let best = peaks.first().expect("a peak should be found");

    assert_eq!(
        best.lag, shift as i64,
        "the peak must point at the true offset"
    );
    assert!(best.ncc > 0.95, "normalised correlation was {}", best.ncc);
}

#[test]
fn ncc_is_symmetric_about_zero_for_a_self_match() {
    let signal = noise(8_000, 0x1234_5678);
    let corr = ncc_correlate(&signal, &signal);
    let peaks = find_peaks(&corr, 1, 5);
    assert_eq!(peaks[0].lag, 0);
    assert!(peaks[0].ncc > 0.99);
}

#[test]
fn ncc_normalisation_removes_a_level_difference() {
    let reference = tones(20_000, 48_000.0);
    let query: Vec<f32> = reference[500..4_500].iter().map(|v| v * 0.01).collect();

    let corr = ncc_correlate(&query, &reference);
    let peaks = find_peaks(&corr, 1, 10);
    assert_eq!(peaks[0].lag, 500);
    // A normalised correlation is insensitive to a pure gain change.
    assert!(
        peaks[0].ncc > 0.95,
        "normalised correlation was {}",
        peaks[0].ncc
    );
}

#[test]
fn ncc_ignores_lags_without_enough_overlap() {
    // The query is much shorter than the reference, so the lags at the very edges of
    // the curve overlap only a fraction of the query. Those lags must not be offered
    // as candidates: over a very short overlap a periodic signal correlates with
    // itself at almost any offset.
    let reference = tones(20_000, 48_000.0);
    let query = &reference[..4_000];
    let corr = ncc_correlate(query, &reference);
    for peak in find_peaks(&corr, 8, 10) {
        assert!(
            corr.is_reliable(peak.lag),
            "unreliable lag {} was offered as a peak",
            peak.lag
        );
    }
}

#[test]
fn gcc_phat_measures_a_known_delay() {
    // A broadband signal: with a handful of pure tones the delay is genuinely
    // ambiguous, because a tone delayed by a whole period looks unchanged.
    let reference = noise(16_000, 0x0bad_c0de);
    let delay = 64usize;
    let mut delayed = vec![0.0f32; delay];
    delayed.extend_from_slice(&reference[..reference.len() - delay]);

    let result = gcc_phat(&reference, &delayed, 512).expect("a delay should be found");
    // `gcc_phat(a, b)` reports the lag at which `b[n + lag]` matches `a[n]`.
    assert!(
        (result.lag - delay as f64).abs() < 1.0,
        "expected ~{delay} samples, got {}",
        result.lag
    );
}

#[test]
fn local_zscore_removes_a_constant_offset() {
    let base: Vec<f32> = (0..500).map(|i| (i as f32 * 0.01).sin() * 10.0).collect();
    let shifted: Vec<f32> = base.iter().map(|v| v + 37.0).collect();

    let a = local_zscore(&base, 50, 4.0);
    let b = local_zscore(&shifted, 50, 4.0);
    for (x, y) in a.iter().zip(b.iter()) {
        assert!((x - y).abs() < 1e-3, "{x} vs {y}");
    }
}

#[test]
fn percentile_is_ordered() {
    let values: Vec<f32> = (0..100).map(|i| i as f32).collect();
    assert_eq!(percentile(&values, 0.0), 0.0);
    assert_eq!(percentile(&values, 1.0), 99.0);
    assert!((percentile(&values, 0.5) - 49.0).abs() < 2.0);
}

#[test]
fn theil_sen_resists_outliers() {
    let mut points: Vec<(f64, f64)> = (0..50)
        .map(|i| {
            let x = i as f64;
            (x, 2.0 * x + 5.0)
        })
        .collect();
    // Ten gross outliers.
    for i in 0..10 {
        points.push((100.0 + i as f64, -500.0));
    }

    let fit = theil_sen(&points).expect("a fit");
    assert!((fit.slope - 2.0).abs() < 1e-9, "slope was {}", fit.slope);
    assert!(
        (fit.intercept - 5.0).abs() < 1e-9,
        "intercept was {}",
        fit.intercept
    );
}

#[test]
fn robust_fit_rejects_outliers() {
    let mut points: Vec<(f64, f64)> = (0..40)
        .map(|i| {
            let x = i as f64;
            (x, 1.00002 * x + 100.0)
        })
        .collect();
    points.push((10.0, 55.0));
    points.push((20.0, 44.0));

    let fit = robust_fit(&points, 3.0, 5).expect("a fit");
    assert!(
        (fit.slope - 1.00002).abs() < 1e-6,
        "slope was {}",
        fit.slope
    );
    assert_eq!(fit.inliers.iter().filter(|i| **i).count(), 40);
}

#[test]
fn ransac_finds_the_inlier_line() {
    let mut points: Vec<(f64, f64)> = (0..60)
        .map(|i| {
            let x = i as f64;
            (x, 3.0 * x - 7.0)
        })
        .collect();
    for i in 0..12 {
        points.push((i as f64 * 5.0, 900.0));
    }

    let fit = ransac(&points, 0.5, 500, 42).expect("a fit");
    assert!((fit.slope - 3.0).abs() < 1e-6, "slope was {}", fit.slope);
}

#[test]
fn least_squares_recovers_an_exact_line() {
    let points: Vec<(f64, f64)> = (0..20).map(|i| (i as f64, 4.0 * i as f64 - 2.0)).collect();
    let fit = least_squares(&points).expect("a fit");
    assert!((fit.slope - 4.0).abs() < 1e-9);
    assert!((fit.intercept + 2.0).abs() < 1e-9);
}

#[test]
fn linear_model_uses_the_documented_convention() {
    let model = LinearModel::new(1.000035, 317.391);
    // reference_time = scale * source_time + offset
    assert!((model.reference_time(0.0) - 317.391).abs() < 1e-9);
    assert!((model.reference_time(1000.0) - 1317.426).abs() < 1e-6);
    assert!((model.drift_ppm() - 35.0).abs() < 1e-9);
    assert!((model.source_time(model.reference_time(123.0)) - 123.0).abs() < 1e-9);
}

#[test]
fn time_map_piecewise_is_continuous() {
    let map = TimeMap::from_segments(vec![
        MapSegment {
            source_start: 0.0,
            source_end: 100.0,
            scale: 1.0,
            reference_start: 10.0,
        },
        MapSegment {
            source_start: 100.0,
            source_end: 200.0,
            scale: 1.001,
            reference_start: 110.0,
        },
    ]);

    assert!((map.reference_time(100.0) - 110.0).abs() < 1e-9);
    assert!((map.reference_time(50.0) - 60.0).abs() < 1e-9);
    assert!((map.reference_time(150.0) - 160.05).abs() < 1e-9);
    assert!(!map.is_linear());
}

#[test]
fn resampler_preserves_amplitude_and_length() {
    let input = tones(48_000, 48_000.0);
    let out = resample_buffer(&input, 1.0, 0.0, 63, 1024);

    assert_eq!(out.len(), input.len());
    let peak_in = input.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let peak_out = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        (peak_in - peak_out).abs() < 0.02,
        "peak changed: {peak_in} -> {peak_out}"
    );
}

#[test]
fn resampler_applies_the_requested_rate_change() {
    // `alpha` is the number of input frames consumed per output frame, so alpha = 2
    // halves the length (a rate reduction) and alpha = 0.5 doubles it.
    let input = tones(48_000, 48_000.0);
    let out = resample_buffer(&input, 2.0, 0.0, 63, 1024);
    assert!(
        (out.len() as i64 - input.len() as i64 / 2).abs() < 4,
        "length was {}",
        out.len()
    );

    let out = resample_buffer(&input, 0.5, 0.0, 63, 1024);
    assert!(
        (out.len() as i64 - input.len() as i64 * 2).abs() < 4,
        "length was {}",
        out.len()
    );
}

#[test]
fn resampler_output_does_not_depend_on_chunking() {
    let input = tones(20_000, 48_000.0);
    let whole = resample_buffer(&input, 0.999_5, 3.25, 63, 1024);

    let mut chunked = StreamingResampler::new(0.999_5, 3.25, 63, 1024);
    chunked.set_output_limit(((input.len() as f64 - 3.25) / 0.999_5).round() as u64);
    let mut pieces = Vec::new();
    for block in input.chunks(1_000) {
        pieces.extend(chunked.push(block));
    }
    pieces.extend(chunked.flush());

    assert_eq!(
        whole.len(),
        pieces.len(),
        "length differs between chunkings"
    );
    let worst = whole
        .iter()
        .zip(pieces.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(worst < 1e-6, "chunked output differs by {worst}");
}

#[test]
fn filter_cutoff_follows_the_rate_ratio() {
    // Upsampling keeps the full input band.
    let up = PolyphaseFilter::design(0.5, 63, 256);
    assert!((up.cutoff() - 0.5).abs() < 1e-9);

    // Downsampling by two halves the cutoff (anti-aliasing).
    let down = PolyphaseFilter::design(2.0, 63, 256);
    assert!((down.cutoff() - 0.25).abs() < 1e-9);
}

#[test]
fn correlation_accessor_handles_out_of_range_lags() {
    let corr = Correlation {
        values: vec![0.1, 0.5, 0.9],
        fixed: vec![0.1, 0.5, 0.9],
        lag_min: -1,
        lag_max: 1,
    };
    assert_eq!(corr.value(-1), 0.1);
    assert_eq!(corr.value(0), 0.5);
    assert_eq!(corr.value(1), 0.9);
    assert_eq!(corr.value(99), 0.0);
}
