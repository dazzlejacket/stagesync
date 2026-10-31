//! Shared fixtures for the StageSync end-to-end tests.
//!
//! Everything here is generated in Rust, inside the test process. The suite depends on
//! nothing outside a temporary directory: no Python, no network, no checked-in media.
//!
//! The ground truth is exact by construction. A source is built by reading the
//! reference at the affine positions
//!
//! ```text
//! reference_sample(j) = scale * (j / out_rate) * ref_rate + offset * ref_rate
//! ```
//!
//! so the correspondence `reference_time = scale * source_time + offset` holds by
//! definition rather than approximately.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use stagesync_core::audio::{WavFormat, WavWriter};
use stagesync_core::matching::AlignmentReport;
use stagesync_core::pipeline::{analyze, AnalysisConfig};

/// A tiny deterministic PRNG (xorshift64*).
///
/// A fixed seed is what makes the whole suite reproducible; nothing here may depend on
/// the system clock, the environment or the iteration order of a hash map.
pub struct Rng(u64);

impl Rng {
    /// A generator with a fixed seed.
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F491_4F6CDD1D)
    }

    /// A uniform value in `[-1, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    /// A uniform value in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next_f32() * 0.5 + 0.5)
    }
}

/// A sine lookup table.
///
/// The fixtures generate millions of samples, and `f32::sin` in a debug build is
/// slow enough to dominate the run time of the suite. A table with linear
/// interpolation is accurate to about 1e-5, which is far beyond what an envelope
/// feature can see.
pub struct SinTable {
    table: Vec<f64>,
}

impl SinTable {
    /// A table of the given size.
    pub fn new(size: usize) -> Self {
        let mut table = Vec::with_capacity(size + 1);
        for i in 0..=size {
            table.push((2.0 * std::f64::consts::PI * i as f64 / size as f64).sin());
        }
        Self { table }
    }

    /// The sine of an angle given in **cycles** (one cycle is a full turn).
    ///
    /// The phase is reduced modulo one *in f64* before the table lookup. Reducing it
    /// after converting to radians would lose the fractional part completely: at a few
    /// thousand cycles an `f32` angle has already rounded away its last few digits,
    /// and the "note" would drift away from where it belongs.
    pub fn sin_cycles(&self, cycles: f64) -> f64 {
        let size = (self.table.len() - 1) as f64;
        let phase = cycles - cycles.floor();
        let pos = phase * size;
        let index = pos as usize;
        let frac = pos - index as f64;
        let a = self.table[index];
        let b = self.table[index + 1];
        a + (b - a) * frac
    }
}

/// Build a deterministic pseudo-musical performance.
///
/// The signal deliberately contains **no exactly repeated passage**: a verbatim-repeating
/// block makes several positions of the recording genuinely indistinguishable, which
/// tests the fixture rather than the matcher. The chord progression does come round
/// again, but every repetition carries different dynamics and a different melody, so it
/// is similar rather than identical.
pub fn performance(duration_secs: f64, rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_secs * rate as f64).round() as usize;
    let mut rng = Rng::new(seed);
    let mut out = vec![0.0f32; n];

    const CHORD_SECS: f64 = 2.3;
    const BEAT_SECS: f64 = 0.5;
    const SECTION_SECS: f64 = 17.0;

    // The chord roots and the per-chord dynamics are drawn from the PRNG into tables
    // that are longer than the fixture, so that the envelope never repeats. A signal
    // with a *periodic* envelope makes many positions look alike, which would test the
    // fixture rather than the matcher.
    let roots: Vec<f64> = (0..256).map(|_| rng.range(110.0, 440.0) as f64).collect();
    let weights: Vec<f64> = (0..256).map(|_| rng.range(0.35, 1.0) as f64).collect();
    let gains: Vec<f64> = (0..64).map(|_| rng.range(0.20, 1.0) as f64).collect();

    let sin = SinTable::new(8192);
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f64 / rate as f64;
        let chord_idx = (t / CHORD_SECS).floor() as usize % roots.len();
        let root = roots[chord_idx];
        let f = root * (1.0 + 0.0007 * chord_idx as f64);

        // Inharmonic partials: harmonically related pure tones share a common period
        // and create exact correlation aliases.
        let mut v = 0.0f64;
        for (h, amp) in [(1.0f64, 0.50), (1.503, 0.30), (2.117, 0.18)] {
            v += amp * sin.sin_cycles(f * h * t);
        }

        // A slowly gliding tone gives the envelope long-term structure.
        let glide = 220.0 + 90.0 * (0.5 + 0.5 * sin.sin_cycles(t / 23.0));
        v += 0.16 * sin.sin_cycles(glide * t);

        let section = (t / SECTION_SECS).floor() as usize % gains.len();
        *slot = (v * weights[chord_idx] * gains[section]) as f32;
    }

    // Percussive bursts: these give the envelope sharp, well-localised features.
    let mut beat = 0usize;
    while beat as f64 * BEAT_SECS < duration_secs {
        let start = (beat as f64 * BEAT_SECS * rate as f64).round() as usize;
        if start >= n {
            break;
        }
        let len = ((0.06 * rate as f64).round() as usize).min(n - start);
        for j in 0..len {
            let env = (1.0 - j as f32 / len as f32).powi(2);
            out[start + j] += env * 0.42 * rng.next_f32();
        }
        beat += 1;
    }

    // A low noise floor, so the material is never digitally silent.
    for v in out.iter_mut() {
        *v += 0.0015 * rng.next_f32();
    }

    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-9);
    for v in out.iter_mut() {
        *v = *v / peak * 0.7;
    }

    // Digital silence at the head and the tail.
    let head = (2.0 * rate as f64).round() as usize;
    let tail = (3.0 * rate as f64).round() as usize;
    for v in out.iter_mut().take(head.min(n)) {
        *v = 0.0;
    }
    for v in out.iter_mut().rev().take(tail.min(n)) {
        *v = 0.0;
    }
    out
}

/// The ground truth of a synthesised source.
#[derive(Debug, Clone, Copy)]
pub struct GroundTruth {
    /// The time scale: `reference_time = scale * source_time + offset`.
    pub scale: f64,
    /// The offset in seconds.
    pub offset: f64,
    /// The source sample rate.
    pub out_rate: u32,
    /// The reference sample rate.
    pub ref_rate: u32,
}

impl GroundTruth {
    /// The reference time that a given source time corresponds to.
    pub fn reference_time(&self, source_time: f64) -> f64 {
        self.scale * source_time + self.offset
    }

    /// The drift in parts per million.
    pub fn drift_ppm(&self) -> f64 {
        (self.scale - 1.0) * 1_000_000.0
    }
}

/// Interpolate a signal at a fractional position with a Hann-windowed sinc.
///
/// This is written here rather than taken from the crate under test: the fixture must
/// not depend on the code it is used to check.
fn sinc_interp(x: &[f32], pos: f64) -> f32 {
    const TAPS: usize = 16;
    const HALF: i64 = (TAPS as i64) / 2 - 1;
    let base = pos.floor() as i64;
    let mut acc = 0.0f64;
    let mut wsum = 0.0f64;
    for k in 0..TAPS {
        let idx = base - HALF + k as i64;
        let d = idx as f64 - pos;
        let sinc = if d.abs() < 1e-12 {
            1.0
        } else {
            (std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d)
        };
        // Hann window.
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / (TAPS - 1) as f64).cos();
        let sample = if idx >= 0 && (idx as usize) < x.len() {
            x[idx as usize] as f64
        } else {
            0.0
        };
        acc += sinc * w * sample;
        wsum += sinc * w;
    }
    if wsum.abs() < 1e-12 {
        0.0
    } else {
        (acc / wsum) as f32
    }
}

/// The parameters of a synthesised source recording.
///
/// A struct rather than a long positional list: every fixture builder takes all of
/// these, and the order of eight arguments is easy to get wrong in a way that still
/// compiles.
#[derive(Debug, Clone, Copy)]
pub struct SourceSpec {
    /// The time scale: `reference_time = scale * source_time + offset`.
    pub scale: f64,
    /// The offset of the source in the reference, in seconds.
    pub offset: f64,
    /// The source's own sample rate.
    pub out_rate: u32,
    /// The level of the source relative to the reference, in dB.
    pub gain_db: f64,
    /// How long the source runs. `None` means "exactly as long as it needs to be to
    /// cover the rest of the reference".
    pub duration_secs: Option<f64>,
}

/// Build a source from a reference at an exact affine correspondence.
///
/// The source starts at reference time `spec.offset`.
///
/// A `duration_secs` of `None` means "cover exactly the rest of the reference", which is
/// what a real recording does: it stops when the performance stops. Leaving the source
/// running past the end of the reference would put a block of digital silence in it,
/// and that silence would corrupt the local z-score of the envelope near the end — the
/// matcher would then be comparing a saturated region against unrelated material.
/// Pass `Some(d)` to make the source deliberately run past the end.
pub fn make_source(
    reference: &[f32],
    ref_rate: u32,
    spec: SourceSpec,
    seed: u64,
) -> (Vec<f32>, GroundTruth) {
    let SourceSpec {
        scale,
        offset,
        out_rate,
        gain_db,
        duration_secs,
    } = spec;
    let reference_duration = reference.len() as f64 / ref_rate as f64;
    let duration = match duration_secs {
        Some(d) => d,
        None => ((reference_duration - offset) / scale).max(0.0),
    };
    let n = (duration * out_rate as f64).round().max(0.0) as usize;

    let gain = 10.0f64.powf(gain_db / 20.0) as f32;
    let mut rng = Rng::new(seed);
    let mut out = vec![0.0f32; n];
    for (j, slot) in out.iter_mut().enumerate() {
        let source_time = j as f64 / out_rate as f64;
        let pos = (scale * source_time + offset) * ref_rate as f64;
        if pos < 0.0 || pos >= reference.len() as f64 {
            // Outside the reference: nothing was recorded there.
            continue;
        }
        *slot = sinc_interp(reference, pos) * gain;
    }
    // A little noise, so the source is not a bit-exact copy of the reference.
    let noise = 0.0008f32 * gain;
    for v in out.iter_mut() {
        *v += noise * rng.next_f32();
    }

    (
        out,
        GroundTruth {
            scale,
            offset,
            out_rate,
            ref_rate,
        },
    )
}

/// Write a mono WAV file.
pub fn write_wav(path: &Path, samples: &[f32], rate: u32, format: WavFormat) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create output directory");
    }
    let mut writer = WavWriter::create(path, rate, 1, format).expect("create WAV writer");
    writer
        .write_planar(std::slice::from_ref(&samples))
        .expect("write samples");
    writer.finalize().expect("finalise WAV");
}

/// Write a stereo WAV file from two planes.
pub fn write_wav_stereo(path: &Path, left: &[f32], right: &[f32], rate: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create output directory");
    }
    let mut writer =
        WavWriter::create(path, rate, 2, WavFormat::Float32).expect("create WAV writer");
    writer.write_planar(&[left, right]).expect("write samples");
    writer.finalize().expect("finalise WAV");
}

/// A cache-key version.
///
/// Bump this whenever the fixture generator changes: a cached file from an older
/// generator would otherwise be compared against a freshly generated source, and the
/// two would not match. That failure looks like a matcher bug, not a stale fixture.
const FIXTURE_VERSION: u64 = 2;

/// The key of the fixture cache: rate, duration in milliseconds, seed and the version
/// of the generator that produced it.
type CacheKey = (u32, u64, u64, u64);

/// The cache of already generated reference files.
type FixtureCache = Mutex<HashMap<CacheKey, PathBuf>>;

/// A cached reference file, so the expensive signal generation happens once per test
/// binary rather than once per test.
fn cache() -> &'static FixtureCache {
    static CACHE: OnceLock<FixtureCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Take the fixture cache lock, ignoring a poisoned lock.
///
/// A panic in one test must not make every later test fail with "called
/// `Result::unwrap()` on an `Err` value" — that would hide the real failure
/// behind a cascade of unrelated ones.
fn cache_lock() -> MutexGuard<'static, HashMap<CacheKey, PathBuf>> {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The directory the cached fixtures live in.
///
/// The directory is **per process**. Two test binaries run at the same time
/// would otherwise generate the same fixture file into the same place, and
/// one of them would read a file the other was still writing. The old shared
/// directory was the reason a cold parallel run failed: the generation was not
/// owned by anybody, so two tests could write the same file at once.
///
/// The directory also carries the clock reading, so a stale directory left
/// behind by a killed process is never reused.
fn cache_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "stagesync-test-fixtures-v{FIXTURE_VERSION}-{pid}-{nanos}"
        ));
        std::fs::create_dir_all(&dir).expect("create the fixture cache directory");
        dir
    })
    .clone()
}

/// A reference WAV file for the given parameters, generated once and reused.
pub fn reference_file(rate: u32, duration_secs: f64, seed: u64) -> PathBuf {
    let key = (rate, (duration_secs * 1000.0) as u64, seed, FIXTURE_VERSION);
    // The lock is held across the generation, not only around the lookup. A
    // lookup that released it before generating left two threads free to
    // generate the same file at the same time.
    let mut cache = cache_lock();
    if let Some(path) = cache.get(&key) {
        return path.clone();
    }
    let path = cache_dir().join(format!(
        "ref_v{FIXTURE_VERSION}_{rate}_{}_{seed}.wav",
        key.1
    ));
    if !path.exists() {
        let signal = performance(duration_secs, rate, seed);
        write_wav(&path, &signal, rate, WavFormat::Float32);
    }
    cache.insert(key, path.clone());
    path
}

/// The reference signal itself, for building a source from it.
pub fn reference_signal(rate: u32, duration_secs: f64, seed: u64) -> Vec<f32> {
    performance(duration_secs, rate, seed)
}

/// An analysis configuration tuned for the test fixtures.
///
/// The log-mel cross-check is off by default: it is a second opinion, it is expensive,
/// and the fixtures are band-limited, so it would cost a lot of time for little signal.
/// One test turns it on explicitly.
pub fn config() -> AnalysisConfig {
    AnalysisConfig {
        compute_log_mel: false,
        ..AnalysisConfig::default()
    }
}

/// Run the analysis over a reference and a list of sources.
pub fn analyse(reference: &Path, sources: &[PathBuf], config: &AnalysisConfig) -> AlignmentReport {
    let mut progress = |_p: stagesync_core::pipeline::Progress| {};
    analyze(reference, sources, config, &mut progress).expect("the analysis should succeed")
}

/// The measured error, in milliseconds, of each trusted match point against the truth.
///
/// Only the envelope points are counted by default: they are the points the model was
/// actually fitted with. The spectral points are an independent second opinion and are
/// not expected to be as accurate — they are compared separately.
pub fn point_errors_ms(
    alignment: &stagesync_core::matching::SourceAlignment,
    truth: &GroundTruth,
) -> Vec<f64> {
    alignment
        .points
        .iter()
        .filter(|p| p.method == stagesync_core::matching::MatchMethod::FineEnvelope)
        .filter(|p| p.confidence.score >= 0.5)
        .map(|p| (p.reference_time - truth.reference_time(p.source_time)) * 1000.0)
        .collect()
}

/// The measured error, in milliseconds, of the spectral cross-check points.
pub fn spectral_errors_ms(
    alignment: &stagesync_core::matching::SourceAlignment,
    truth: &GroundTruth,
) -> Vec<f64> {
    alignment
        .points
        .iter()
        .filter(|p| p.method == stagesync_core::matching::MatchMethod::FineSpectral)
        .filter(|p| p.confidence.score >= 0.5)
        .map(|p| (p.reference_time - truth.reference_time(p.source_time)) * 1000.0)
        .collect()
}

/// Print a summary of an analysis, so a failing test says what actually happened.
pub fn describe(report: &AlignmentReport) {
    for alignment in &report.sources {
        println!(
            "source {}: fine {} measured / {} skipped, points {}",
            alignment.source.file_name,
            alignment.fine_measured,
            alignment.fine_skipped,
            alignment.points.len()
        );
        if let Some(coarse) = &alignment.coarse {
            println!(
                "  coarse locked={} reason={:?} full_lag={:?} full_score={:.4}",
                coarse.locked, coarse.reason, coarse.full_range_lag_secs, coarse.full_range_score
            );
            for candidate in &coarse.candidates {
                println!(
                    "    candidate lag {:.3} s, {} vote(s), mean score {:.4}",
                    candidate.lag_secs, candidate.votes, candidate.mean_score
                );
            }
        }
        for warning in &alignment.warnings {
            println!("  warning: {warning}");
        }
    }
}

/// p50, p95 and the maximum absolute value of a residual series.
pub fn stats(values: &[f64]) -> (f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted: Vec<f64> = values.iter().map(|v| v.abs()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = sorted[sorted.len() / 2];
    let p95 = sorted[((sorted.len() as f64 * 0.95).round() as usize).min(sorted.len() - 1)];
    let max = sorted[sorted.len() - 1];
    (p50, p95, max)
}

/// Print an accuracy summary, so a failing run says what actually went wrong.
pub fn report_accuracy(label: &str, values: &[f64]) {
    let (p50, p95, max) = stats(values);
    println!(
        "{label}: n={} p50={:.3} ms p95={:.3} ms max={:.3} ms",
        values.len(),
        p50,
        p95,
        max
    );
}
