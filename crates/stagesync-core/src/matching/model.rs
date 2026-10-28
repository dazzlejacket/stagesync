//! The time model: `reference_time = scale * source_time + offset`.
//!
//! A single linear model describes a recorder whose clock ran at a slightly different rate
//! than the reference. When the evidence shows that the rate changed *during* the
//! recording, the model becomes piecewise: several linear segments joined continuously, so
//! that no material is stretched or warped across a boundary.

use serde::{Deserialize, Serialize};

/// A linear clock model.
///
/// `scale` is the ratio of the source clock to the reference clock; `offset` is the
/// reference time that corresponds to source time zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearModel {
    /// Time scaling factor: `reference_time = scale * source_time + offset`.
    pub scale: f64,
    /// Offset in seconds.
    pub offset: f64,
}

impl LinearModel {
    /// Construct a model.
    pub fn new(scale: f64, offset: f64) -> Self {
        Self { scale, offset }
    }

    /// Drift of the source clock relative to the reference, in parts per million.
    pub fn drift_ppm(&self) -> f64 {
        (self.scale - 1.0) * 1_000_000.0
    }

    /// The pitch change the correction implies, in cents.
    ///
    /// Resampling by `alpha = 1/scale` changes the playback rate by `scale`, so every
    /// frequency moves by the same factor and the pitch shifts by `-1200 * log2(scale)`.
    /// A positive drift therefore pitches the material *down*.
    pub fn pitch_shift_cents(&self) -> f64 {
        -1200.0 * self.scale.log2()
    }

    /// Reference time corresponding to a source time.
    pub fn reference_time(&self, source_time: f64) -> f64 {
        self.scale * source_time + self.offset
    }

    /// Source time corresponding to a reference time.
    pub fn source_time(&self, reference_time: f64) -> f64 {
        (reference_time - self.offset) / self.scale
    }

    /// Accumulated drift between two source times, in milliseconds.
    pub fn accumulated_drift_ms(&self, source_start: f64, source_end: f64) -> f64 {
        self.drift_ppm() / 1_000_000.0 * (source_end - source_start) * 1000.0
    }
}

/// One continuous piece of a (possibly piecewise) time map.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapSegment {
    /// Start of the segment in source seconds.
    pub source_start: f64,
    /// End of the segment in source seconds.
    pub source_end: f64,
    /// Time scale within the segment.
    pub scale: f64,
    /// Reference time at `source_start`. Segments are joined continuously.
    pub reference_start: f64,
}

impl MapSegment {
    /// Reference time for a source time inside this segment.
    pub fn reference_time(&self, source_time: f64) -> f64 {
        self.reference_start + self.scale * (source_time - self.source_start)
    }

    /// Source time for a reference time inside this segment.
    pub fn source_time(&self, reference_time: f64) -> f64 {
        self.source_start + (reference_time - self.reference_start) / self.scale
    }
}

/// A (possibly piecewise) continuous time map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeMap {
    /// Segments, ordered by `source_start`, joined continuously.
    pub segments: Vec<MapSegment>,
}

impl TimeMap {
    /// Build a single-segment map covering `[0, source_duration]`.
    pub fn from_linear(model: LinearModel, source_duration: f64) -> Self {
        Self {
            segments: vec![MapSegment {
                source_start: 0.0,
                source_end: source_duration,
                scale: model.scale,
                reference_start: model.offset,
            }],
        }
    }

    /// Build a map from explicit segments (each carries its own `reference_start`).
    pub fn from_segments(segments: Vec<MapSegment>) -> Self {
        Self { segments }
    }

    /// The segment containing `source_time`.
    pub fn segment_at(&self, source_time: f64) -> Option<&MapSegment> {
        self.segments
            .iter()
            .find(|s| source_time >= s.source_start && source_time < s.source_end)
            .or_else(|| {
                // Fall back to the closest segment at the boundaries.
                self.segments.iter().min_by(|a, b| {
                    let da = (a.source_start - source_time).abs();
                    let db = (b.source_start - source_time).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
            })
    }

    /// Reference time corresponding to a source time.
    pub fn reference_time(&self, source_time: f64) -> f64 {
        match self.segment_at(source_time) {
            Some(s) => s.reference_time(source_time),
            None => source_time,
        }
    }

    /// Source time corresponding to a reference time.
    pub fn source_time(&self, reference_time: f64) -> f64 {
        self.segments
            .iter()
            .find(|s| {
                let r_end = s.reference_start + s.scale * (s.source_end - s.source_start);
                reference_time >= s.reference_start && reference_time < r_end
            })
            .map(|s| s.source_time(reference_time))
            .unwrap_or_else(|| {
                // Outside the mapped span: extrapolate from the nearest end.
                if let Some(first) = self.segments.first() {
                    if reference_time < first.reference_start {
                        return first.source_time(reference_time);
                    }
                }
                if let Some(last) = self.segments.last() {
                    return last.source_time(reference_time);
                }
                reference_time
            })
    }

    /// Time scale at a source time.
    pub fn scale_at(&self, source_time: f64) -> f64 {
        self.segment_at(source_time).map_or(1.0, |s| s.scale)
    }

    /// Drift of the first segment in ppm (the headline drift figure).
    pub fn drift_ppm(&self) -> f64 {
        self.segments
            .first()
            .map_or(0.0, |s| (s.scale - 1.0) * 1_000_000.0)
    }

    /// Drift range across all segments, in ppm.
    pub fn drift_range_ppm(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for s in &self.segments {
            let ppm = (s.scale - 1.0) * 1_000_000.0;
            lo = lo.min(ppm);
            hi = hi.max(ppm);
        }
        if lo.is_finite() {
            (lo, hi)
        } else {
            (0.0, 0.0)
        }
    }

    /// The equivalent single linear model, when the map is in fact linear.
    pub fn linear(&self) -> Option<LinearModel> {
        if self.segments.len() != 1 {
            return None;
        }
        let s = &self.segments[0];
        Some(LinearModel::new(s.scale, s.reference_start))
    }

    /// Source-time span covered by the map.
    pub fn source_span(&self) -> (f64, f64) {
        match (self.segments.first(), self.segments.last()) {
            (Some(f), Some(l)) => (f.source_start, l.source_end),
            _ => (0.0, 0.0),
        }
    }

    /// Reference-time span covered by the map.
    pub fn reference_span(&self) -> (f64, f64) {
        match (self.segments.first(), self.segments.last()) {
            (Some(f), Some(l)) => {
                let end = l.reference_start + l.scale * (l.source_end - l.source_start);
                (f.reference_start, end)
            }
            _ => (0.0, 0.0),
        }
    }

    /// `true` when the map describes a constant rate.
    pub fn is_linear(&self) -> bool {
        self.segments.len() == 1
    }
}
