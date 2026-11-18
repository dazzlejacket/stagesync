//! How a recording is placed in the stereo field.
//!
//! Panning is a **matrix**: the recording's own channels are mixed into the output's
//! channels with fixed coefficients. Nothing is ever concatenated, and no channel of
//! a recording is ever silently replaced by another.
//!
//! Three pan modes exist, because a stereo recording and a mono recording are
//! different problems:
//!
//! * A **mono** recording is a point in the field. It is placed with a pan law:
//!   constant power (the level is kept as it moves) or constant gain (the sum of the
//!   two gains is one at every position).
//! * A **stereo** recording is an *image*. The default is **balance**: the channels
//!   are attenuated towards the side the recording is panned to, and at the centre
//!   the recording is untouched — its width, its phase relationship and its level are
//!   exactly what they were.
//! * A stereo recording can also be panned with the **stereo-aware** mode, which
//!   moves the image across the field while keeping the width, and **folded to mono**,
//!   which is the one mode that really does lose the stereo image and says so.

use serde::{Deserialize, Serialize};

/// The law used to place a **mono** recording in the stereo field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PanLaw {
    /// Constant power: the recording keeps its level as it moves across the field.
    ///
    /// The gains are the cosine and the sine of the pan angle, so the *sum of the
    /// squares* of the two gains is one at every position. A recording panned to the
    /// centre is 3 dB below the same recording panned to one side, which is the price
    /// of a constant-power law and the reason it is the default.
    #[default]
    EqualPower,

    /// Constant gain: the sum of the two gains is one at every position.
    ///
    /// A recording panned to the centre is 6 dB down in each side, and one panned to a
    /// side is at its own level in that side and silent in the other. The recording's
    /// total amplitude is the same wherever it is; what changes is how much of it
    /// reaches each side. It suits material that is already a single voice in a
    /// stereo field.
    ConstantGain,
}

/// How a **stereo** recording is placed in the stereo field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StereoPan {
    /// The channels are attenuated towards the side the recording is panned to.
    ///
    /// At the centre the recording is **untouched**: the left channel keeps its gain
    /// of one and the right channel keeps its gain of one, so the image is exactly
    /// what it was. Towards a side the far channel is attenuated, which narrows the
    /// image — that is what a balance control does, and the narrowing is the point.
    #[default]
    Balance,

    /// The image moves across the field and the width is kept.
    ///
    /// The recording is split into its mid (the part both channels share) and its
    /// side (the part that differs between them). The mid is placed in the field; the
    /// side is scaled by how far the recording is from the centre, so the width
    /// shrinks towards a side but the image moves rather than merely narrowing.
    ///
    /// At the centre the matrix is the identity: the recording is bit for bit what it
    /// was.
    ///
    /// A recording whose channels are in antiphase has no mid at all, so there is no
    /// image to move: this mode can only narrow such a recording. The balance mode
    /// moves it by attenuating the far channel instead.
    StereoAware,

    /// The recording is folded to mono and panned as a mono recording.
    ///
    /// This is the one mode that really does lose the stereo image. It is offered
    /// because it is sometimes what is wanted — two wide recordings summed together
    /// can sound enormous — but it is never applied by default, and the report says
    /// when a recording was folded.
    MonoFold,
}

/// A matrix that maps a recording's channels onto the output's channels.
///
/// `rows[output_channel][input_channel]` is the gain from the input channel to the
/// output channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanMatrix {
    /// The matrix itself.
    rows: [[f32; 2]; 2],
    /// How many of the recording's channels the matrix reads.
    input_channels: usize,
}

impl PanMatrix {
    /// The matrix for a recording with the given number of channels.
    ///
    /// `pan` is `-1` for hard left and `+1` for hard right. A recording with more
    /// than two channels is treated as two: the extra channels are not silently
    /// dropped by the matrix, but the renderer refuses such a recording outright.
    pub fn for_source(channels: usize, pan: f32, law: PanLaw, stereo: StereoPan) -> Self {
        let pan = pan.clamp(-1.0, 1.0);
        if channels <= 1 {
            let (left, right) = mono_gains(pan, law);
            return Self {
                rows: [[left, 0.0], [right, 0.0]],
                input_channels: 1,
            };
        }
        let rows = match stereo {
            StereoPan::Balance => {
                let (left, right) = balance_gains(pan);
                [[left, 0.0], [0.0, right]]
            }
            StereoPan::StereoAware => stereo_aware_matrix(pan),
            StereoPan::MonoFold => {
                let (left, right) = mono_gains(pan, law);
                [[0.5 * left, 0.5 * left], [0.5 * right, 0.5 * right]]
            }
        };
        Self {
            rows,
            input_channels: 2,
        }
    }

    /// The matrix that folds a recording to mono, ignoring the pan.
    ///
    /// A pan control has nothing to do when there is only one output channel, so a
    /// mono mixdown ignores it rather than pretending to apply it.
    pub fn fold(channels: usize) -> Self {
        if channels <= 1 {
            Self {
                rows: [[1.0, 0.0], [0.0, 0.0]],
                input_channels: 1,
            }
        } else {
            Self {
                rows: [[0.5, 0.5], [0.0, 0.0]],
                input_channels: 2,
            }
        }
    }

    /// How many of the recording's channels the matrix reads.
    pub fn input_channels(&self) -> usize {
        self.input_channels
    }

    /// Mix the recording's channels into the output's two channels.
    pub fn apply(&self, input: &[f32], output: &mut [f32; 2]) {
        let left = input.first().copied().unwrap_or(0.0);
        let right = input.get(1).copied().unwrap_or(0.0);
        output[0] = self.rows[0][0] * left + self.rows[0][1] * right;
        output[1] = self.rows[1][0] * left + self.rows[1][1] * right;
    }

    /// The gain from an input channel to an output channel.
    pub fn gain(&self, output: usize, input: usize) -> f32 {
        self.rows[output][input]
    }

    /// The gains applied to the recording's own left and right channels, for a
    /// recording that goes to the output's left and right channels unchanged.
    ///
    /// This is only meaningful for the balance mode; the other modes mix the
    /// channels, so there is no single gain per channel.
    pub fn diagonal(&self) -> Option<(f32, f32)> {
        if self.rows[0][1] == 0.0 && self.rows[1][0] == 0.0 {
            Some((self.rows[0][0], self.rows[1][1]))
        } else {
            None
        }
    }
}

/// The gains that place a **mono** recording in the field.
///
/// The equal-power law puts the recording at the same *power* in the field wherever
/// it is; the constant-gain law puts it at the same *amplitude*.
pub fn mono_gains(pan: f32, law: PanLaw) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    match law {
        PanLaw::EqualPower => {
            let angle = ((pan + 1.0) as f64 * std::f64::consts::FRAC_PI_4) as f32;
            (angle.cos(), angle.sin())
        }
        PanLaw::ConstantGain => {
            // The gains add up to one at every position, so the recording's total
            // amplitude does not change as it moves. A pan of -1 is hard **left**:
            // the sign is what says which side, and it is not optional.
            ((1.0 - pan) * 0.5, (1.0 + pan) * 0.5)
        }
    }
}

/// The gains that balance a **stereo** recording towards one side.
///
/// At the centre both gains are one, so the recording is untouched.
pub fn balance_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    (1.0 - pan.max(0.0), 1.0 - (-pan).max(0.0))
}

/// The matrix that moves a stereo recording's image while keeping its width.
///
/// The recording is split into mid and side:
///
/// ```text
/// mid  = (left + right) / 2
/// side = (left - right) / 2
/// ```
///
/// The mid is placed with a constant-gain balance towards the panned side, and the
/// side is scaled by `1 - |pan|`, so the width shrinks towards a side but the image
/// *moves* rather than merely narrowing.
///
/// At the centre the matrix is the identity: the recording is unchanged.
pub fn stereo_aware_matrix(pan: f32) -> [[f32; 2]; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    let (balance_left, balance_right) = balance_gains(pan);
    let side = 1.0 - pan.abs();
    [
        [0.5 * (balance_left + side), 0.5 * (balance_left - side)],
        [0.5 * (balance_right - side), 0.5 * (balance_right + side)],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The matrix that applies to a given recording, pan and mode.
    fn matrix(channels: usize, pan: f32, stereo: StereoPan) -> PanMatrix {
        PanMatrix::for_source(channels, pan, PanLaw::EqualPower, stereo)
    }

    #[test]
    fn a_stereo_recording_at_the_centre_is_untouched() {
        for mode in [StereoPan::Balance, StereoPan::StereoAware] {
            let m = matrix(2, 0.0, mode);
            let mut out = [0.0f32; 2];
            m.apply(&[0.3, -0.7], &mut out);
            assert!(
                (out[0] - 0.3).abs() < 1e-6 && (out[1] + 0.7).abs() < 1e-6,
                "the {mode:?} mode changed the recording at the centre: {out:?}"
            );
        }
    }

    #[test]
    fn the_balance_mode_never_mixes_the_channels_of_a_recording() {
        let m = matrix(2, 0.5, StereoPan::Balance);
        assert_eq!(m.diagonal(), Some((0.5, 1.0)));
        let mut out = [0.0f32; 2];
        m.apply(&[1.0, 1.0], &mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_stereo_aware_mode_moves_the_image_and_keeps_some_width() {
        // A wide recording whose channels agree in sign: the left is louder than the
        // right, so there is an image to move.
        let wide = [0.5f32, 0.25f32];
        let centre = {
            let mut out = [0.0f32; 2];
            matrix(2, 0.0, StereoPan::StereoAware).apply(&wide, &mut out);
            out
        };
        let right = {
            let mut out = [0.0f32; 2];
            matrix(2, 0.6, StereoPan::StereoAware).apply(&wide, &mut out);
            out
        };
        // The image has moved to the right.
        assert!(
            right[1] > right[0],
            "panning right did not move the image: {right:?}"
        );
        // The width has not vanished: the two channels still differ.
        let width = |v: [f32; 2]| (v[0] - v[1]).abs();
        assert!(
            width(right) > 0.1 * width(centre),
            "the stereo-aware mode destroyed the width: {right:?} against {centre:?}"
        );
    }

    #[test]
    fn the_stereo_aware_mode_narrows_an_antiphase_recording_without_moving_it() {
        // An antiphase recording has no mid, so there is no image to move: the mode
        // can only narrow it. This is stated rather than hidden, because it is the
        // difference between the balance mode and this one.
        let antiphase = [0.5f32, -0.5f32];
        let centre = {
            let mut out = [0.0f32; 2];
            matrix(2, 0.0, StereoPan::StereoAware).apply(&antiphase, &mut out);
            out
        };
        let right = {
            let mut out = [0.0f32; 2];
            matrix(2, 0.6, StereoPan::StereoAware).apply(&antiphase, &mut out);
            out
        };
        assert!(
            (right[0] + right[1]).abs() < 1e-6,
            "the antiphase pair stopped being in antiphase: {right:?}"
        );
        assert!(
            right[0].abs() < centre[0].abs(),
            "the antiphase pair was not narrowed: {right:?} against {centre:?}"
        );
    }

    #[test]
    fn the_equal_power_law_keeps_the_power_of_a_mono_recording() {
        for pan in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            let (left, right) = mono_gains(pan, PanLaw::EqualPower);
            let power = left * left + right * right;
            assert!(
                (power - 1.0).abs() < 1e-5,
                "the power at a pan of {pan} was {power}"
            );
        }
    }

    #[test]
    fn the_constant_gain_law_keeps_the_amplitude_of_a_mono_recording() {
        for pan in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            let (left, right) = mono_gains(pan, PanLaw::ConstantGain);
            assert!(
                ((left + right) - 1.0).abs() < 1e-6,
                "the amplitude at a pan of {pan} was {}",
                left + right
            );
        }
    }

    #[test]
    fn a_hard_panned_stereo_recording_lands_on_one_side_only() {
        let mut out = [0.0f32; 2];
        matrix(2, 1.0, StereoPan::StereoAware).apply(&[0.4, 0.6], &mut out);
        assert!(
            out[0].abs() < 1e-6,
            "the left channel was not silent: {out:?}"
        );
        assert!(
            (out[1] - 0.5).abs() < 1e-6,
            "the right channel was wrong: {out:?}"
        );
    }

    #[test]
    fn an_antiphase_recording_keeps_its_sign() {
        // A recording whose channels are in antiphase: folding it to mono would
        // produce silence, which is why the fold is never the default.
        let antiphase = [0.5f32, -0.5f32];
        let mut out = [0.0f32; 2];
        matrix(2, 0.0, StereoPan::Balance).apply(&antiphase, &mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[1] + 0.5).abs() < 1e-6);
        assert!(
            out[0].abs() > 1e-6,
            "the antiphase recording was folded to silence"
        );
    }

    #[test]
    fn the_fold_ignores_the_pan() {
        let m = PanMatrix::fold(2);
        let mut out = [0.0f32; 2];
        m.apply(&[1.0, 3.0], &mut out);
        assert!((out[0] - 2.0).abs() < 1e-6);
        assert!(out[1].abs() < 1e-9);
    }
}
