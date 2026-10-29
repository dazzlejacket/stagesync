//! Deterministic variable-rate resampling by polyphase windowed-sinc interpolation.
//!
//! Correcting a clock drift of a few tens of parts per million changes the *effective
//! playback rate* of the recording. The only honest way to apply that change while keeping
//! the material transparent is to resample — and the resampler must be good enough that
//! the correction is audibly invisible.
//!
//! The design here is the classic Crochiere–Rabiner polyphase interpolator:
//!
//! * the kernel is a windowed sinc, Kaiser-windowed for ~90 dB of stop-band rejection;
//! * the cutoff follows the *actual* output/input rate ratio, so the filter also acts as
//!   an anti-alias filter when the rate is reduced;
//! * the filter bank is pre-computed once and interpolated between phases, which makes
//!   the per-sample cost a short dot product;
//! * the output sample position is computed from an exact linear map, so the result does
//!   not depend on chunking: the same input always yields bit-identical output.
//!
//! No time-stretching (WSOLA, phase vocoder) is performed anywhere: a few ppm of rate
//! change is inaudible, and stretching would smear transients for no benefit.

use std::collections::VecDeque;

/// Default number of filter taps (odd, so the kernel is symmetric about a sample).
pub const DEFAULT_TAPS: usize = 63;
/// Default number of interpolation phases.
pub const DEFAULT_PHASES: usize = 1024;

/// A pre-computed polyphase filter bank.
#[derive(Debug, Clone)]
pub struct PolyphaseFilter {
    /// Filter coefficients, phase-major: `bank[phase * taps + tap]`.
    bank: Vec<f32>,
    taps: usize,
    phases: usize,
    /// Cutoff in cycles per input sample.
    cutoff: f64,
}

impl PolyphaseFilter {
    /// Design a filter for a given step (input frames advanced per output frame).
    ///
    /// `step` is the ratio of the output sample rate to the input sample rate expressed
    /// in input frames: `step = fs_in / fs_out`.
    pub fn design(step: f64, taps: usize, phases: usize) -> Self {
        let taps = if taps.is_multiple_of(2) {
            taps + 1
        } else {
            taps
        };
        let half = taps / 2;
        let step = if step.abs() < 1e-12 { 1.0 } else { step.abs() };

        // Anti-alias cutoff: the output Nyquist expressed in cycles per input sample,
        // never above the input Nyquist.
        let cutoff = 0.5 * (1.0 / step).min(1.0);

        let mut bank = vec![0.0f32; phases * taps];
        for p in 0..phases {
            let frac = p as f64 / phases as f64;
            let mut sum = 0.0f64;
            for j in 0..taps {
                // Offset of this tap from the interpolation position.
                let d = frac + half as f64 - j as f64;
                let h = 2.0 * cutoff * sinc(2.0 * cutoff * d) * kaiser(d / half as f64);
                bank[p * taps + j] = h as f32;
                sum += h;
            }
            // Normalise so that DC is preserved exactly.
            if sum.abs() > 1e-12 {
                for j in 0..taps {
                    bank[p * taps + j] /= sum as f32;
                }
            }
        }

        Self {
            bank,
            taps,
            phases,
            cutoff,
        }
    }

    /// Number of taps per phase.
    pub fn taps(&self) -> usize {
        self.taps
    }

    /// Cutoff in cycles per input sample.
    pub fn cutoff(&self) -> f64 {
        self.cutoff
    }

    /// Interpolate the filter row for a fractional position.
    #[inline]
    fn row(&self, frac: f64) -> (&[f32], &[f32], f32) {
        let q = (frac * self.phases as f64).clamp(0.0, self.phases as f64 - 1e-9);
        let i0 = q.floor() as usize % self.phases;
        let i1 = (i0 + 1) % self.phases;
        let w = (q - q.floor()) as f32;
        (
            &self.bank[i0 * self.taps..(i0 + 1) * self.taps],
            &self.bank[i1 * self.taps..(i1 + 1) * self.taps],
            w,
        )
    }
}

/// A streaming resampler with an exact linear position map.
///
/// Output frame `n` is taken from input position `alpha * n + gamma`.
pub struct StreamingResampler {
    filter: PolyphaseFilter,
    alpha: f64,
    gamma: f64,
    half: usize,
    /// Buffered input samples; `buffer_start` is the absolute index of the first sample.
    buffer: VecDeque<f32>,
    buffer_start: i64,
    /// Next output frame index to produce.
    next_output: i64,
    /// Index of the last input sample pushed.
    input_end: i64,
    /// Total number of output samples produced so far.
    produced: u64,
    /// Maximum number of output samples to produce; `None` means "as many as the input
    /// supports", which includes the filter tail.
    output_limit: Option<u64>,
}

impl StreamingResampler {
    /// Create a resampler for the map `p(n) = alpha * n + gamma`.
    pub fn new(alpha: f64, gamma: f64, taps: usize, phases: usize) -> Self {
        let filter = PolyphaseFilter::design(alpha, taps, phases);
        let half = filter.taps() / 2;
        Self {
            filter,
            alpha,
            gamma,
            half,
            buffer: VecDeque::new(),
            buffer_start: 0,
            next_output: 0,
            input_end: 0,
            produced: 0,
            output_limit: None,
        }
    }

    /// The filter in use.
    pub fn filter(&self) -> &PolyphaseFilter {
        &self.filter
    }

    /// Start producing output at frame `n`.
    ///
    /// The position map is absolute (`p(n) = alpha * n + gamma`), so jumping to a later
    /// output frame simply skips the work; it does not change the result. This is how a
    /// timeline render skips the leading silence without generating it sample by sample.
    pub fn set_next_output(&mut self, n: i64) {
        self.next_output = n;
    }

    /// The next output frame index that will be produced.
    pub fn next_output(&self) -> i64 {
        self.next_output
    }

    /// Stop after producing `n` output samples.
    ///
    /// A resampler whose input has been fully consumed still holds the tail of its
    /// filter; without a limit the output would be longer than the rate change alone
    /// requires. The renderer sets the limit so that the output length is exactly
    /// `round(input_length * alpha)`.
    pub fn set_output_limit(&mut self, n: u64) {
        self.output_limit = Some(n);
    }

    /// Number of output samples produced so far.
    pub fn produced(&self) -> u64 {
        self.produced
    }

    /// Push input samples and return every output sample that can now be produced.
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        self.buffer.extend(samples.iter().copied());
        self.input_end += samples.len() as i64;
        self.drain(false)
    }

    /// Finish the stream, producing the remaining outputs (the tail of the kernel).
    pub fn flush(&mut self) -> Vec<f32> {
        self.drain(true)
    }

    fn drain(&mut self, flush: bool) -> Vec<f32> {
        let mut out = Vec::new();
        let taps = self.filter.taps();
        loop {
            if let Some(limit) = self.output_limit {
                if self.produced >= limit {
                    break;
                }
            }
            let n = self.next_output;
            let p = self.alpha * n as f64 + self.gamma;
            let base = p.floor() as i64 - self.half as i64;
            let needed_end = base + taps as i64;

            if needed_end > self.input_end {
                if !flush {
                    break;
                }
                // At the end of the stream, zero-pad beyond the input.
                if base > self.input_end {
                    break;
                }
            }

            // Fetch the tap window, zero-padding outside the available input.
            let buf = self.buffer.make_contiguous();
            let mut acc = 0.0f32;
            let frac = p - p.floor();
            let (c0, c1, w) = self.filter.row(frac);
            for j in 0..taps {
                let idx = base + j as i64 - self.buffer_start;
                let x = if idx < 0 {
                    0.0
                } else if (idx as usize) < buf.len() {
                    buf[idx as usize]
                } else {
                    0.0
                };
                let c = c0[j] + w * (c1[j] - c0[j]);
                acc += c * x;
            }
            out.push(acc);
            self.next_output += 1;
            self.produced += 1;
        }

        // Discard input samples that can no longer be needed.
        let next_base =
            (self.alpha * self.next_output as f64 + self.gamma).floor() as i64 - self.half as i64;
        let drop = (next_base - self.buffer_start).max(0) as usize;
        if drop > 0 {
            self.buffer.drain(..drop.min(self.buffer.len()));
            self.buffer_start += drop as i64;
        }
        out
    }
}

/// Resample a complete buffer in one call (convenience wrapper).
///
/// `alpha` is the number of input frames advanced per output frame, so the natural
/// output length is `round(input.len() * alpha)`.
pub fn resample_buffer(
    input: &[f32],
    alpha: f64,
    gamma: f64,
    taps: usize,
    phases: usize,
) -> Vec<f32> {
    let limit = (((input.len() as f64) - gamma) / alpha).round().max(0.0) as u64;
    let mut r = StreamingResampler::new(alpha, gamma, taps, phases);
    r.set_output_limit(limit);
    let mut out = r.push(input);
    out.extend(r.flush());
    out
}

/// The `sinc` function: `sin(pi x) / (pi x)`.
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let pix = std::f64::consts::PI * x;
        pix.sin() / pix
    }
}

/// Kaiser window value for a normalised distance `r` in `[0, 1]`.
fn kaiser(r: f64) -> f64 {
    const BETA: f64 = 8.6; // ~90 dB stop-band attenuation
    let r = r.abs();
    if r >= 1.0 {
        return 0.0;
    }
    let arg = BETA * (1.0 - r * r).sqrt();
    bessel_i0(arg) / bessel_i0(BETA)
}

/// Modified Bessel function of the first kind, order zero (series expansion).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let y = x * x / 4.0;
    for k in 1..64 {
        term *= y / (k as f64 * k as f64);
        sum += term;
        if term < 1e-16 * sum {
            break;
        }
    }
    sum
}
