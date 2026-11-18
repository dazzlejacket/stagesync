//! Levels: meters, clipping protection and loudness matching.
//!
//! Three different things are often confused with one another, and the mixer keeps
//! them apart:
//!
//! * **Peak protection** keeps the *written file* inside the range its format can
//!   represent. It says how much gain it took.
//! * **Loudness matching** makes the *recordings* comparable with one another. It is
//!   a change to the material and it is always the user's decision.
//! * **Metering** only *reports*. It changes nothing at all.
//!
//! A recording that was already clipped when it was made cannot be un-clipped by
//! turning the volume down: the peaks are gone from the file. Nothing here claims
//! otherwise, and the mixer never presents a gain reduction as though it had
//! restored anything.

use serde::{Deserialize, Serialize};

/// A level in decibels relative to full scale.
pub type Db = f64;

/// Convert decibels to a linear gain.
pub fn db_to_linear(db: Db) -> f32 {
    if db <= -200.0 {
        return 0.0;
    }
    10f64.powf(db / 20.0) as f32
}

/// Convert a linear amplitude to decibels relative to full scale.
///
/// Zero is reported as minus infinity, which is what it is: silence has no level.
pub fn linear_to_db(amplitude: f32) -> Db {
    if amplitude <= 0.0 || !amplitude.is_finite() {
        return f64::NEG_INFINITY;
    }
    20.0 * (amplitude as f64).log10()
}

/// How the mix protects itself against clipping.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ClippingProtection {
    /// Nothing is done.
    ///
    /// A 32-bit float file can hold a sample above 0 dBFS, so a peak above one is
    /// written as it is and can still be turned down afterwards without any loss.
    /// A 16-bit file cannot: the sample is clipped at the format's limit, and the
    /// number of samples that were clipped is reported.
    #[default]
    None,

    /// A soft knee above the threshold.
    ///
    /// The signal is left alone below the threshold and is compressed smoothly
    /// above it, so the output never exceeds full scale. This **does** change the
    /// material, and the number of samples it touched is reported.
    SoftClip {
        /// The level at which the soft knee begins, as a linear amplitude.
        threshold: f32,
    },

    /// One master gain, chosen so that the loudest sample of the mix lands on the
    /// target.
    ///
    /// The whole mix is measured first and then written with a single gain, so the
    /// relative levels of the recordings are untouched. It costs a second pass over
    /// the material, and the gain it applied is reported.
    AutoGain {
        /// The level the loudest sample of the mix should land on, in dBFS.
        target_dbfs: Db,
    },
}

impl ClippingProtection {
    /// The gain the protection applies, given the loudest sample of the mix.
    ///
    /// Only the automatic gain changes the level of the mix as a whole; the soft
    /// knee changes the shape of the peaks instead.
    pub fn master_gain(self, peak: f32) -> f32 {
        match self {
            ClippingProtection::AutoGain { target_dbfs } => {
                if peak <= 0.0 {
                    return 1.0;
                }
                let target = db_to_linear(target_dbfs);
                if peak <= target {
                    1.0
                } else {
                    target / peak
                }
            }
            _ => 1.0,
        }
    }

    /// The gain reduction the protection applies, in decibels.
    ///
    /// The number is negative when the mix was turned down.
    pub fn gain_reduction_db(self, peak: f32) -> Db {
        let gain = self.master_gain(peak);
        linear_to_db(gain)
    }

    /// The level the automatic gain aims at, in dBFS.
    ///
    /// The other protections have no target, so they report the level they leave the
    /// mix at.
    pub fn target_dbfs(self) -> Db {
        match self {
            ClippingProtection::AutoGain { target_dbfs } => target_dbfs,
            ClippingProtection::SoftClip { threshold } => linear_to_db(threshold),
            ClippingProtection::None => 0.0,
        }
    }

    /// The soft knee applied to one sample.
    pub fn apply(self, sample: f32, master_gain: f32) -> f32 {
        let value = sample * master_gain;
        match self {
            ClippingProtection::SoftClip { threshold } => soft_clip(value, threshold),
            _ => value,
        }
    }
}

/// Compress a sample smoothly above the threshold.
///
/// Below the threshold the sample is untouched. Above it, the excess is compressed
/// towards the threshold with a hyperbolic tangent, so the output approaches full
/// scale asymptotically and never exceeds it.
pub fn soft_clip(sample: f32, threshold: f32) -> f32 {
    let threshold = threshold.clamp(0.0, 1.0);
    let magnitude = sample.abs();
    if magnitude <= threshold || threshold >= 1.0 {
        return sample;
    }
    let headroom = 1.0 - threshold;
    let excess = (magnitude - threshold) / headroom;
    let compressed = threshold + headroom * excess.tanh();
    sample.signum() * compressed
}

/// Whether the recordings are matched to a common level before they are mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LoudnessMatch {
    /// No matching: every recording keeps its own level.
    ///
    /// This is the default, and it is the behaviour of every other export in
    /// StageSync: a recording that is 37 dB quieter than the reference stays 37 dB
    /// quieter.
    #[default]
    None,

    /// Every recording is brought to the level of the loudest one.
    ///
    /// The level is measured as the **root-mean-square** of the recording over its
    /// whole length. This is not a perceptual loudness measurement: it applies no
    /// frequency weighting, so it does not account for the way the ear weights
    /// different octaves. It is called loudness matching because that is what the
    /// user asked for, and it is reported as an RMS match so that nobody is misled
    /// about what was measured.
    MatchToLoudest,
}

/// A peak meter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PeakMeter {
    /// The loudest sample the meter has seen.
    pub peak: f32,
    /// How many samples above 0 dBFS the meter has seen.
    pub overs: u64,
}

impl PeakMeter {
    /// A meter that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a sample to the meter.
    pub fn observe(&mut self, sample: f32) {
        let magnitude = sample.abs();
        if magnitude > self.peak {
            self.peak = magnitude;
        }
        if magnitude > 1.0 {
            self.overs += 1;
        }
    }

    /// The loudest sample the meter has seen, in dBFS.
    pub fn peak_dbfs(&self) -> Db {
        linear_to_db(self.peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_below_full_scale_is_untouched_by_the_soft_knee() {
        assert_eq!(soft_clip(0.5, 0.9), 0.5);
        assert_eq!(soft_clip(-0.5, 0.9), -0.5);
    }

    #[test]
    fn the_soft_knee_never_exceeds_full_scale() {
        for sample in [1.0f32, 2.0, 10.0, 1000.0] {
            let out = soft_clip(sample, 0.9);
            assert!(out <= 1.0, "the soft knee produced {out}");
            assert!(out > 0.9, "the soft knee did nothing: {out}");
        }
    }

    #[test]
    fn the_soft_knee_keeps_the_sign() {
        assert!(soft_clip(-3.0, 0.9) < 0.0);
        assert!(soft_clip(3.0, 0.9) > 0.0);
    }

    #[test]
    fn the_automatic_gain_brings_the_peak_onto_the_target() {
        let protection = ClippingProtection::AutoGain { target_dbfs: -1.0 };
        let gain = protection.master_gain(2.0);
        assert!((2.0 * gain - db_to_linear(-1.0)).abs() < 1e-6);
        assert!(protection.gain_reduction_db(2.0) < 0.0);
    }

    #[test]
    fn the_automatic_gain_leaves_a_quiet_mix_alone() {
        let protection = ClippingProtection::AutoGain { target_dbfs: -1.0 };
        assert_eq!(protection.master_gain(0.5), 1.0);
        assert_eq!(protection.gain_reduction_db(0.5), 0.0);
    }

    #[test]
    fn the_meter_counts_the_samples_above_full_scale() {
        let mut meter = PeakMeter::new();
        meter.observe(0.5);
        meter.observe(1.5);
        meter.observe(-2.0);
        assert_eq!(meter.overs, 2);
        assert_eq!(meter.peak, 2.0);
    }

    #[test]
    fn silence_has_no_level() {
        assert!(linear_to_db(0.0).is_infinite());
        assert!(linear_to_db(1.0).abs() < 1e-9);
    }
}
