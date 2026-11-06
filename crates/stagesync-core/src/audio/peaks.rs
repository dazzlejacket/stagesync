//! Multiresolution waveform peaks, built by streaming.
//!
//! A waveform display needs two things that pull in opposite directions: the **whole**
//! recording has to be visible, and the **detail** of the material has to be visible
//! too. Holding the samples would satisfy the second and destroy the first — a
//! three-hour stereo recording at 48 kHz is four gigabytes of `f32`, and no desktop
//! application should put that in its memory, let alone in the graphical interface's.
//!
//! So the recording is reduced to **peaks**: for every bucket of samples, the lowest
//! and the highest sample in it. A bucket is short enough that the picture keeps its
//! shape and long enough that the picture is cheap. The peaks are kept at several
//! resolutions at once, so that zooming in does not need the file to be read again:
//! the finest level answers "what does this millisecond look like" and the coarsest
//! answers "what does the whole show look like".
//!
//! The file is **streamed** while the cache is built. The decoder hands over one block
//! at a time and the block is folded into the peaks and dropped, so the memory the
//! cache needs does not depend on the length of the recording.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

use super::decode::AudioDecoder;

/// Frames per bucket in the finest level of the cache.
///
/// 512 frames is about eleven milliseconds at 48 kHz: fine enough for a waveform to
/// keep its shape, coarse enough that a three-hour stereo recording's finest level
/// costs a few tens of megabytes rather than gigabytes.
const BASE_BUCKET: u64 = 512;

/// How many times coarser each level is than the one below it.
const LEVEL_FACTOR: u64 = 4;

/// The coarsest level stops once this few buckets remain.
///
/// The coarsest level is the one the whole show is drawn from. A handful of buckets
/// is enough for that, and stopping there keeps the levels that follow from costing
/// anything.
const MINIMUM_BUCKETS: u64 = 32;

/// One resolution of the peak cache.
#[derive(Debug, Clone)]
pub struct PeakLevel {
    /// How many frames each bucket covers.
    pub bucket: u64,
    /// The lowest sample in each bucket, one plane per channel.
    pub mins: Vec<Vec<f32>>,
    /// The highest sample in each bucket, one plane per channel.
    pub maxs: Vec<Vec<f32>>,
}

impl PeakLevel {
    /// How many buckets this level holds.
    pub fn buckets(&self) -> usize {
        self.mins.first().map_or(0, |plane| plane.len())
    }
}

/// The peaks of a recording, at several resolutions.
#[derive(Debug, Clone)]
pub struct PeakCache {
    /// The sample rate of the recording.
    pub rate: u32,
    /// How many channels the recording has.
    pub channels: usize,
    /// How many frames the recording has, per channel.
    pub frames: u64,
    /// The levels, finest first.
    pub levels: Vec<PeakLevel>,
}

impl PeakCache {
    /// Build the peak cache of a recording by streaming it.
    ///
    /// The file is read once, from beginning to end, and never held in memory. The
    /// optional cancellation flag is checked between blocks, so a cancelled build
    /// stops within a block rather than at the end of the recording.
    pub fn build(path: &Path, cancel: Option<&AtomicBool>) -> Result<PeakCache> {
        let mut decoder = AudioDecoder::open(path)
            .with_context(|| format!("failed to open {} for peak extraction", path.display()))?;
        let rate = decoder.meta().sample_rate;
        let channels = decoder.meta().channels.max(1);

        // The accumulator for the finest level. Every channel keeps its own bucket
        // boundary, and all of them see the same frame indices, so the levels stay
        // aligned with one another.
        let mut mins: Vec<Vec<f32>> = vec![Vec::new(); channels];
        let mut maxs: Vec<Vec<f32>> = vec![Vec::new(); channels];
        let mut current_min = vec![f32::INFINITY; channels];
        let mut current_max = vec![f32::NEG_INFINITY; channels];
        let mut current_bucket = vec![u64::MAX; channels];
        let mut total_frames: u64 = 0;

        decoder
            .for_each_block(8192, |block| {
                if let Some(flag) = cancel {
                    if flag.load(Ordering::Relaxed) {
                        return Err(anyhow::anyhow!("the peak extraction was cancelled"));
                    }
                }
                for (channel, plane) in block.planes.iter().enumerate() {
                    if channel >= channels {
                        break;
                    }
                    let start = block.start_frame;
                    for (index, &sample) in plane.iter().enumerate() {
                        let bucket = (start + index as u64) / BASE_BUCKET;
                        if bucket != current_bucket[channel] {
                            // The bucket is finished: its extremes are filed away
                            // and the next one starts.
                            if current_bucket[channel] != u64::MAX {
                                mins[channel].push(current_min[channel]);
                                maxs[channel].push(current_max[channel]);
                            }
                            current_bucket[channel] = bucket;
                            current_min[channel] = sample;
                            current_max[channel] = sample;
                        } else {
                            if sample < current_min[channel] {
                                current_min[channel] = sample;
                            }
                            if sample > current_max[channel] {
                                current_max[channel] = sample;
                            }
                        }
                    }
                }
                total_frames = block.start_frame + block.frames() as u64;
                Ok(())
            })
            .with_context(|| format!("failed to read {} for peak extraction", path.display()))?;

        // The last bucket of every channel is filed away as well.
        for channel in 0..channels {
            if current_bucket[channel] != u64::MAX {
                mins[channel].push(current_min[channel]);
                maxs[channel].push(current_max[channel]);
            }
        }

        let mut levels = vec![PeakLevel {
            bucket: BASE_BUCKET,
            mins,
            maxs,
        }];
        // Each level is the one below it, reduced.
        while levels.last().map_or(0, |level| level.buckets() as u64) > MINIMUM_BUCKETS {
            let previous = levels.last().expect("there is a level");
            levels.push(reduce(previous));
        }

        Ok(PeakCache {
            rate,
            channels,
            frames: total_frames,
            levels,
        })
    }

    /// The length of the recording, in seconds.
    pub fn duration_secs(&self) -> f64 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames as f64 / self.rate as f64
        }
    }

    /// The peaks covering a span of the recording.
    ///
    /// The level is chosen so that the span is covered by no more than `max_buckets`
    /// buckets: the finest level that fits. When the recording is zoomed all the way
    /// out, that is the coarsest level, and the whole show is drawn from a few
    /// thousand numbers rather than from four gigabytes of samples.
    ///
    /// `None` when the cache is empty.
    pub fn peaks(&self, start_secs: f64, end_secs: f64, max_buckets: usize) -> Option<PeakWindow> {
        if self.levels.is_empty() || self.rate == 0 {
            return None;
        }
        let max_buckets = max_buckets.max(1) as u64;
        let start_frame = (start_secs.max(0.0) * self.rate as f64).floor() as u64;
        let end_frame = (end_secs.max(0.0) * self.rate as f64).ceil() as u64;

        // The finest level that covers the span with no more than `max_buckets`
        // buckets.
        let mut chosen = &self.levels[self.levels.len() - 1];
        for level in &self.levels {
            let first = start_frame / level.bucket;
            let last = end_frame.div_ceil(level.bucket);
            if last.saturating_sub(first) <= max_buckets {
                chosen = level;
                break;
            }
        }

        let first = (start_frame / chosen.bucket) as usize;
        let last = (end_frame.div_ceil(chosen.bucket) as usize).min(chosen.buckets());
        let count = last.saturating_sub(first);
        if count == 0 {
            return None;
        }

        let mut mins = Vec::with_capacity(self.channels);
        let mut maxs = Vec::with_capacity(self.channels);
        for channel in 0..self.channels {
            let min_plane = chosen.mins.get(channel);
            let max_plane = chosen.maxs.get(channel);
            mins.push(
                min_plane
                    .map(|plane| plane[first..last].to_vec())
                    .unwrap_or_default(),
            );
            maxs.push(
                max_plane
                    .map(|plane| plane[first..last].to_vec())
                    .unwrap_or_default(),
            );
        }

        Some(PeakWindow {
            start_secs: start_frame as f64 / self.rate as f64,
            end_secs: end_frame as f64 / self.rate as f64,
            bucket_secs: chosen.bucket as f64 / self.rate as f64,
            mins,
            maxs,
            buckets: count,
        })
    }
}

/// Reduce a level by folding every `LEVEL_FACTOR` buckets into one.
fn reduce(level: &PeakLevel) -> PeakLevel {
    let factor = LEVEL_FACTOR as usize;
    let buckets = level.buckets();
    let count = buckets.div_ceil(factor);
    let mut mins: Vec<Vec<f32>> = vec![Vec::with_capacity(count); level.mins.len()];
    let mut maxs: Vec<Vec<f32>> = vec![Vec::with_capacity(count); level.maxs.len()];

    for channel in 0..level.mins.len() {
        let min_plane = &level.mins[channel];
        let max_plane = &level.maxs[channel];
        for bucket in 0..count {
            let first = bucket * factor;
            let last = ((bucket + 1) * factor).min(buckets);
            let mut low = f32::INFINITY;
            let mut high = f32::NEG_INFINITY;
            for index in first..last {
                let m = min_plane.get(index).copied().unwrap_or(0.0);
                let x = max_plane.get(index).copied().unwrap_or(0.0);
                if m < low {
                    low = m;
                }
                if x > high {
                    high = x;
                }
            }
            mins[channel].push(if low.is_finite() { low } else { 0.0 });
            maxs[channel].push(if high.is_finite() { high } else { 0.0 });
        }
    }

    PeakLevel {
        bucket: level.bucket * LEVEL_FACTOR,
        mins,
        maxs,
    }
}

/// A window of peaks, ready to be drawn.
#[derive(Debug, Clone)]
pub struct PeakWindow {
    /// The time the window starts at, in seconds.
    pub start_secs: f64,
    /// The time the window ends at, in seconds.
    pub end_secs: f64,
    /// How many seconds one bucket covers.
    pub bucket_secs: f64,
    /// The lowest sample in each bucket, one plane per channel.
    pub mins: Vec<Vec<f32>>,
    /// The highest sample in each bucket, one plane per channel.
    pub maxs: Vec<Vec<f32>>,
    /// How many buckets the window holds.
    pub buckets: usize,
}

impl PeakWindow {
    /// The lowest and the highest sample of the window, across every channel.
    ///
    /// A waveform is drawn from the extremes of all its channels, so that a channel
    /// that is quieter than the others is still visible.
    pub fn extremes(&self) -> (f32, f32) {
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for channel in 0..self.mins.len() {
            for &value in &self.mins[channel] {
                if value < low {
                    low = value;
                }
            }
            for &value in &self.maxs[channel] {
                if value > high {
                    high = value;
                }
            }
        }
        if low.is_finite() && high.is_finite() {
            (low, high)
        } else {
            (0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A tone whose amplitude rises and falls, written to a temporary file.
    fn fixture(dir: &std::path::Path, rate: u32, frames: usize) -> PathBuf {
        let path = dir.join("peaks.wav");
        let samples: Vec<f32> = (0..frames)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (t * 40.0).sin() * (0.2 + 0.8 * (t / 2.0).min(1.0))
            })
            .collect();
        let mut writer = super::super::wav::WavWriter::create(
            &path,
            rate,
            1,
            super::super::wav::WavFormat::Float32,
        )
        .expect("create the writer");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write the samples");
        writer.finalize().expect("finalise the file");
        path
    }

    #[test]
    fn the_peaks_cover_the_whole_recording() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000;
        let frames = rate as usize * 3;
        let path = fixture(dir.path(), rate, frames);

        let cache = PeakCache::build(&path, None).expect("the cache");
        assert_eq!(cache.rate, rate);
        assert_eq!(cache.channels, 1);
        assert_eq!(cache.frames, frames as u64);
        assert!((cache.duration_secs() - 3.0).abs() < 1e-9);
        assert!(!cache.levels.is_empty());
    }

    #[test]
    fn a_zoomed_out_window_uses_the_coarse_level() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000;
        let path = fixture(dir.path(), rate, rate as usize * 60);

        let cache = PeakCache::build(&path, None).expect("the cache");
        let window = cache.peaks(0.0, 60.0, 1000).expect("a window");
        assert!(
            window.buckets <= 1000,
            "the window held {} buckets",
            window.buckets
        );
        assert!(
            window.bucket_secs > 0.01,
            "the zoomed-out window used a bucket of {} s",
            window.bucket_secs
        );
    }

    #[test]
    fn a_zoomed_in_window_keeps_the_shape_of_the_material() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000;
        let path = fixture(dir.path(), rate, rate as usize * 10);

        let cache = PeakCache::build(&path, None).expect("the cache");
        let window = cache.peaks(1.0, 1.5, 500).expect("a window");
        let finest = BASE_BUCKET as f64 / rate as f64;
        assert!(
            window.bucket_secs <= finest + 1e-9,
            "the zoomed-in window used a bucket of {} s, and the finest level is {finest} s",
            window.bucket_secs
        );
        // The tone's amplitude grows over the first two seconds, so the window
        // must contain both quiet and loud material.
        let (low, high) = window.extremes();
        assert!(low <= -0.15, "the quiet part is missing: {low}");
        assert!(high >= 0.15, "the loud part is missing: {high}");
    }

    #[test]
    fn the_coarse_levels_are_the_reduction_of_the_fine_one() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000;
        let path = fixture(dir.path(), rate, rate as usize * 20);

        let cache = PeakCache::build(&path, None).expect("the cache");
        assert!(cache.levels.len() >= 2);
        let fine = &cache.levels[0];
        let coarse = &cache.levels[1];
        assert_eq!(coarse.bucket, fine.bucket * LEVEL_FACTOR);
        assert_eq!(
            coarse.buckets(),
            fine.buckets().div_ceil(LEVEL_FACTOR as usize)
        );

        // The extremes of the whole recording must survive the reduction.
        let (low, high) = cache.peaks(0.0, 20.0, 4096).expect("a window").extremes();
        assert!(low <= -0.3 && high >= 0.3, "the extremes are {low}..{high}");
    }
}
