//! Waveform-domain drift estimation.
//!
//! The envelope matcher measures the *shape* of the loudness over time. That is a
//! robust way to find out *where* a recording sits on the reference timeline, but it is
//! a noisy way to find out *how fast* the recorder's clock ran: an envelope probe
//! locates a transient to within a millisecond or two, and a drift of a few tens of
//! parts per million over a few minutes is itself only a few milliseconds.
//!
//! This module measures the same quantity on the **raw waveform** instead. The
//! estimator is different in kind — it correlates the audio itself rather than its
//! loudness — so its noise is different too, and much smaller when the two files
//! really do contain the same signal.
//!
//! The estimator is deliberately conservative. Two different microphones recording the
//! same performance have *uncorrelated* waveforms but strongly correlated envelopes, so
//! a waveform measurement that does not show a high normalised correlation is not a
//! measurement of anything. Such a result is reported, and rejected.
//!
//! The reference is decoded **once** for every source that is analysed, no matter how
//! many sources there are: the windows are cut out of a single streaming pass.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::drift::{fit_with_uncertainty, SlopeFit};
use super::dsp::gcc_phat;
use super::model::LinearModel;
use super::regression::robust_fit;
use crate::audio::windows::{extract_windows, WindowRequest};

/// Configuration for the waveform-domain drift estimator.
#[derive(Debug, Clone, Copy)]
pub struct WaveformOptions {
    /// The rate the comparison is performed at.
    pub analysis_rate: u32,
    /// The length of each comparison window, in seconds.
    pub window_secs: f64,
    /// How far either side of the expected position the search reaches, in seconds.
    pub search_secs: f64,
    /// The largest number of probes to measure per source.
    pub max_probes: usize,
    /// The minimum normalised correlation a probe must reach to be counted.
    pub min_correlation: f32,
    /// The minimum peak separation a probe must reach to be counted.
    pub min_zscore: f32,
}

impl Default for WaveformOptions {
    fn default() -> Self {
        Self {
            analysis_rate: 8_000,
            window_secs: 6.0,
            search_secs: 0.25,
            max_probes: 24,
            min_correlation: 0.30,
            min_zscore: 6.0,
        }
    }
}

/// One waveform-domain measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformProbe {
    /// Position in the source, in seconds.
    pub source_time: f64,
    /// The measured position in the reference, in seconds.
    pub reference_time: f64,
    /// The normalised correlation of the raw waveform at the measured lag.
    pub correlation: f32,
    /// How many standard deviations the phase peak stood above the sidelobes.
    pub zscore: f32,
    /// Whether the measurement was good enough to be used.
    pub counted: bool,
    /// Why the measurement was rejected, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The outcome of a waveform-domain measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformDrift {
    /// Every probe, counted or not.
    pub probes: Vec<WaveformProbe>,
    /// The fit over the counted probes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<SlopeFit>,
    /// The model the fit describes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<LinearModel>,
    /// The mean normalised correlation of the counted probes.
    pub mean_correlation: f32,
    /// Notes about the measurement.
    pub notes: Vec<String>,
}

impl WaveformDrift {
    /// How many probes were counted.
    pub fn counted(&self) -> usize {
        self.probes.iter().filter(|p| p.counted).count()
    }
}

/// The source-side windows of one probe set.
pub type SourceWindows = Vec<Option<Vec<f32>>>;

/// The reference-side windows of one probe set.
pub type ReferenceWindows = Vec<Option<Vec<f32>>>;

/// Cut the source windows out of the source file, in one streaming pass.
pub fn extract_source_windows(
    source_path: &Path,
    positions: &[(f64, f64)],
    options: &WaveformOptions,
    cancel: Option<&AtomicBool>,
) -> Result<SourceWindows> {
    let requests: Vec<WindowRequest> = positions
        .iter()
        .map(|(source_time, _)| WindowRequest::centred(*source_time, options.window_secs))
        .collect();
    extract_windows(source_path, &requests, options.analysis_rate, cancel)
}

/// Cut the reference windows for **every** source out of the reference file.
///
/// One pass over the reference serves every source, which matters: the reference is
/// the longest file in the set, and decoding it once per source would make the cost of
/// the analysis grow with the number of sources for no reason.
pub fn extract_reference_windows(
    reference_path: &Path,
    groups: &[Vec<(f64, f64)>],
    options: &WaveformOptions,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<ReferenceWindows>> {
    // The reference window is longer than the source window on both sides, so the
    // correlation has material to work with at every lag the search can reach.
    let reference_width = options.window_secs + 2.0 * options.search_secs;
    let mut all: Vec<WindowRequest> = Vec::new();
    let mut boundaries: Vec<(usize, usize)> = Vec::with_capacity(groups.len());
    for group in groups {
        let start = all.len();
        all.extend(
            group
                .iter()
                .map(|(_, centre)| WindowRequest::centred(*centre, reference_width)),
        );
        boundaries.push((start, all.len()));
    }
    let windows = extract_windows(reference_path, &all, options.analysis_rate, cancel)?;
    Ok(boundaries
        .into_iter()
        .map(|(start, end)| windows[start..end].to_vec())
        .collect())
}

/// Correlate one source window against one reference window and measure the lag.
pub fn correlate(
    source_window: &[f32],
    reference_window: &[f32],
    centre: f64,
    options: &WaveformOptions,
) -> Option<WaveformProbe> {
    let rate = options.analysis_rate as f64;
    let max_lag = ((2.0 * options.search_secs + 0.05) * rate).round() as usize;
    let phat = gcc_phat(source_window, reference_window, max_lag)?;

    // `a[n] ≈ b[n + lag]`, so the source window's centre sits at
    // `centre - search + lag / rate` in the reference.
    let reference_time = centre - options.search_secs + phat.lag / rate;
    let correlation = normalised_correlation(source_window, reference_window, phat.lag);

    let mut note = None;
    let mut counted = true;
    if correlation < options.min_correlation {
        counted = false;
        note = Some(format!(
            "the raw waveforms are not the same signal (correlation {correlation:.2})"
        ));
    } else if phat.zscore < options.min_zscore {
        counted = false;
        note = Some(format!(
            "the correlation peak was not distinct enough (z {:.1})",
            phat.zscore
        ));
    }

    Some(WaveformProbe {
        source_time: 0.0,
        reference_time,
        correlation,
        zscore: phat.zscore,
        counted,
        note,
    })
}

/// Measure the drift of one source against the reference on the raw waveform.
///
/// `positions` gives, for each probe, the source position and the reference position
/// the search is centred on. The centre only has to be close: the search reaches
/// `options.search_secs` either side of it, and the peak decides the rest.
///
/// The probes are expected to sit at positions that were **not** used to fit the
/// envelope model, so the result is an independent measurement of the drift rather
/// than a restatement of the model.
pub fn measure_drift(
    source_path: &Path,
    reference_path: &Path,
    positions: &[(f64, f64)],
    options: &WaveformOptions,
    cancel: Option<&AtomicBool>,
) -> Result<WaveformDrift> {
    let source_windows = extract_source_windows(source_path, positions, options, cancel)?;
    let reference_windows =
        extract_reference_windows(reference_path, &[positions.to_vec()], options, cancel)?;
    let reference_windows = reference_windows.into_iter().next().unwrap_or_default();
    measure_from_windows(positions, &source_windows, &reference_windows, options)
}

/// Measure the drift from windows that have already been extracted.
pub fn measure_from_windows(
    positions: &[(f64, f64)],
    source_windows: &[Option<Vec<f32>>],
    reference_windows: &[Option<Vec<f32>>],
    options: &WaveformOptions,
) -> Result<WaveformDrift> {
    let mut notes = Vec::new();
    if positions.is_empty() {
        notes.push("no hold-out positions were available to probe".to_string());
        return Ok(WaveformDrift {
            probes: Vec::new(),
            fit: None,
            model: None,
            mean_correlation: 0.0,
            notes,
        });
    }

    let mut probes = Vec::with_capacity(positions.len());
    for (index, (source_time, centre)) in positions.iter().enumerate() {
        let (Some(source_window), Some(reference_window)) = (
            source_windows.get(index).and_then(|w| w.as_ref()),
            reference_windows.get(index).and_then(|w| w.as_ref()),
        ) else {
            probes.push(WaveformProbe {
                source_time: *source_time,
                reference_time: *centre,
                correlation: 0.0,
                zscore: 0.0,
                counted: false,
                note: Some("the window could not be extracted from the audio".to_string()),
            });
            continue;
        };

        match correlate(source_window, reference_window, *centre, options) {
            Some(mut probe) => {
                probe.source_time = *source_time;
                probes.push(probe);
            }
            None => probes.push(WaveformProbe {
                source_time: *source_time,
                reference_time: *centre,
                correlation: 0.0,
                zscore: 0.0,
                counted: false,
                note: Some("the correlation produced no peak".to_string()),
            }),
        }
    }

    let counted_probes: Vec<(f64, f64)> = probes
        .iter()
        .filter(|p| p.counted)
        .map(|p| (p.source_time, p.reference_time))
        .collect();

    let mean_correlation = if counted_probes.is_empty() {
        0.0
    } else {
        let total: f64 = probes
            .iter()
            .filter(|p| p.counted)
            .map(|p| p.correlation as f64)
            .sum();
        (total / counted_probes.len() as f64) as f32
    };

    // A robust fit over the counted probes, then the uncertainty of that fit.
    let fit = if counted_probes.len() >= 3 {
        let robust = robust_fit(&counted_probes, 3.0, 6);
        let inliers: Vec<(f64, f64)> = match &robust {
            Some(fit) => counted_probes
                .iter()
                .zip(fit.inliers.iter())
                .filter(|(_, ok)| **ok)
                .map(|((x, y), _)| (*x, *y))
                .collect(),
            None => counted_probes.clone(),
        };
        fit_with_uncertainty(&inliers)
    } else {
        None
    };

    if counted_probes.is_empty() {
        notes.push(
            "no waveform probe produced a usable correlation, so the raw waveforms do not appear \
             to be the same signal"
                .to_string(),
        );
    }

    Ok(WaveformDrift {
        model: fit.map(|f| f.model()),
        probes,
        fit,
        mean_correlation,
        notes,
    })
}

/// The normalised correlation of two signals at a given lag.
///
/// This is an ordinary correlation coefficient, in `[-1, 1]`, and it is what tells a
/// genuine same-signal match apart from a coincidental one. Two independent
/// microphones produce a value near zero however well their envelopes line up.
fn normalised_correlation(a: &[f32], b: &[f32], lag: f64) -> f32 {
    let lag = lag.round() as i64;
    // `a[n] ≈ b[n + lag]`: the overlap runs over the indices where both exist.
    let lo = (-lag).max(0) as usize;
    let hi = ((b.len() as i64 - lag) as usize).min(a.len());
    if hi <= lo {
        return 0.0;
    }
    let mut sum_a = 0.0f64;
    let mut sum_b = 0.0f64;
    for i in lo..hi {
        sum_a += a[i] as f64;
        sum_b += b[(i as i64 + lag) as usize] as f64;
    }
    let count = (hi - lo) as f64;
    let mean_a = sum_a / count;
    let mean_b = sum_b / count;

    let mut num = 0.0f64;
    let mut energy_a = 0.0f64;
    let mut energy_b = 0.0f64;
    for i in lo..hi {
        let x = a[i] as f64 - mean_a;
        let y = b[(i as i64 + lag) as usize] as f64 - mean_b;
        num += x * y;
        energy_a += x * x;
        energy_b += y * y;
    }
    let denominator = (energy_a * energy_b).sqrt();
    if denominator > 1e-12 {
        (num / denominator) as f32
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_correlates_perfectly_with_itself() {
        let signal: Vec<f32> = (0..2_000)
            .map(|i| ((i as f64 * 0.37).sin() * 0.5) as f32)
            .collect();
        let correlation = normalised_correlation(&signal, &signal, 0.0);
        assert!(
            correlation > 0.999,
            "a signal does not correlate with itself: {correlation}"
        );
    }

    #[test]
    fn an_unrelated_signal_does_not_correlate() {
        let a: Vec<f32> = (0..2_000)
            .map(|i| ((i as f64 * 0.37).sin() * 0.5) as f32)
            .collect();
        let b: Vec<f32> = (0..2_000)
            .map(|i| ((i as f64 * 1.91).sin() * 0.5) as f32)
            .collect();
        let correlation = normalised_correlation(&a, &b, 0.0);
        assert!(
            correlation.abs() < 0.2,
            "unrelated signals correlated: {correlation}"
        );
    }

    #[test]
    fn the_measurement_finds_a_known_shift() {
        // A copy of the signal, delayed by 100 samples, correlates at that delay.
        let signal: Vec<f32> = (0..4_000)
            .map(|i| ((i as f64 * 0.21).sin() * 0.5) as f32)
            .collect();
        let mut shifted = vec![0.0f32; 100];
        shifted.extend_from_slice(&signal);
        shifted.truncate(signal.len() + 200);

        let correlation = normalised_correlation(&signal, &shifted, 100.0);
        assert!(correlation > 0.99, "the shift was not found: {correlation}");
    }
}
