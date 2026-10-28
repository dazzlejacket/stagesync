//! Segmentation: what is actually covered, what is missing, and what is merely uncertain.
//!
//! The central rule of this project is that **missing material is reported, never
//! fabricated**. A span of the reference that no recorder captured is an explicit gap; a
//! span where the match is weak is an explicit low-confidence span. Neither is ever
//! stretched, interpolated or force-aligned to make a timeline look continuous.

use serde::{Deserialize, Serialize};

use super::model::TimeMap;
use super::{Confidence, MatchPoint, Segment, SegmentKind, UncertaintyReason};
use crate::features::Envelope;

/// A span on the merged reference timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentCoverage {
    /// Start in reference seconds.
    pub reference_start: f64,
    /// End in reference seconds.
    pub reference_end: f64,
    /// What this span represents.
    pub kind: SegmentKind,
    /// Which source owns it, if any.
    pub source_index: Option<usize>,
    /// Confidence across the span.
    pub confidence: Confidence,
    /// Human-readable note.
    pub note: String,
}

/// Alias kept for readability at the call site.
pub type TimelineSpan = SegmentCoverage;

/// Tunables for segmentation.
#[derive(Debug, Clone, Copy)]
pub struct SegmentOptions {
    /// A source-time gap longer than this (seconds) between two confident probes is
    /// treated as a genuine break rather than a missed probe.
    pub gap_threshold_secs: f64,
    /// Maximum probe spacing expected in continuous material, in seconds.
    pub expected_probe_spacing_secs: f64,
    /// A gap is only reported as a *recorder gap* when the source really is silent.
    pub gap_requires_silence: bool,
}

impl Default for SegmentOptions {
    fn default() -> Self {
        Self {
            gap_threshold_secs: 5.0,
            expected_probe_spacing_secs: 45.0,
            gap_requires_silence: true,
        }
    }
}

/// Coverage statistics for one source against the reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    /// Reference seconds covered with a trustworthy match.
    pub covered_seconds: f64,
    /// Reference seconds covered but only by extrapolation or weak matches.
    pub low_confidence_seconds: f64,
    /// Reference seconds with no coverage from this source.
    pub uncovered_seconds: f64,
    /// Fraction of the reference covered, in `[0, 1]`.
    pub fraction: f64,
}

/// Analyse one source against the reference and produce its segments.
pub fn analyse_source(
    source_index: usize,
    map: &TimeMap,
    points: &[MatchPoint],
    source_env: &Envelope,
    source_duration: f64,
    reference_duration: f64,
    options: &SegmentOptions,
) -> (Vec<Segment>, Coverage) {
    let mut segments: Vec<Segment> = Vec::new();

    let mut sorted: Vec<&MatchPoint> = points.iter().collect();
    sorted.sort_by(|a, b| a.source_time.partial_cmp(&b.source_time).unwrap());

    let trusted: Vec<&MatchPoint> = sorted
        .iter()
        .copied()
        .filter(|p| p.confidence.is_trustworthy())
        .collect();

    if trusted.is_empty() {
        segments.push(Segment {
            kind: SegmentKind::UncoveredReference,
            source_index: Some(source_index),
            source_start: None,
            source_end: None,
            reference_start: 0.0,
            reference_end: reference_duration,
            confidence: Confidence::new(0.0, UncertaintyReason::LowPeakSeparation),
            note: "no trustworthy correspondence was found for this source".into(),
        });
        return (
            segments,
            Coverage {
                covered_seconds: 0.0,
                low_confidence_seconds: 0.0,
                uncovered_seconds: reference_duration,
                fraction: 0.0,
            },
        );
    }

    let first = trusted[0];
    let last = trusted[trusted.len() - 1];

    // A span between two trusted probes is only a *gap* when it is much longer than the
    // probe spacing itself. A fixed threshold is wrong: with a 15 s probe hop every
    // inter-probe span would look like a gap, and the timeline would be reported as a
    // string of holes instead of continuous coverage. The threshold therefore follows
    // the spacing the matcher actually achieved.
    let spacing = median_probe_spacing(&trusted, options.expected_probe_spacing_secs);
    let gap_threshold = options.gap_threshold_secs.max(2.5 * spacing);

    // Leading region: source material before the first confident probe.
    if first.source_time > 0.5 {
        let ref_start = map.reference_time(0.0);
        let ref_end = map.reference_time(first.source_time);
        if ref_end > ref_start {
            segments.push(Segment {
                kind: SegmentKind::LowConfidence,
                source_index: Some(source_index),
                source_start: Some(0.0),
                source_end: Some(first.source_time),
                reference_start: ref_start,
                reference_end: ref_end,
                confidence: Confidence::new(0.3, UncertaintyReason::LowEnergy),
                note: format!(
                    "source material before the first confident match ({:.1} s); placed by model extrapolation",
                    first.source_time
                ),
            });
        }
    }

    // Trailing region: source material after the last confident probe.
    if source_duration - last.source_time > 0.5 {
        let ref_start = map.reference_time(last.source_time);
        let ref_end = map.reference_time(source_duration);
        if ref_end > ref_start {
            segments.push(Segment {
                kind: SegmentKind::LowConfidence,
                source_index: Some(source_index),
                source_start: Some(last.source_time),
                source_end: Some(source_duration),
                reference_start: ref_start,
                reference_end: ref_end,
                confidence: Confidence::new(0.3, UncertaintyReason::LowEnergy),
                note: format!(
                    "source material after the last confident match ({:.1} s); placed by model extrapolation",
                    source_duration - last.source_time
                ),
            });
        }
    }

    // Spans between consecutive confident probes.
    for window in trusted.windows(2) {
        let a = window[0];
        let b = window[1];
        let src_gap = b.source_time - a.source_time;
        let ref_start = a.reference_time;
        let ref_end = b.reference_time;

        if src_gap > gap_threshold {
            let silent = source_silent_between(source_env, a.source_time, b.source_time);
            let kind = if options.gap_requires_silence && silent {
                SegmentKind::RecorderGap
            } else {
                SegmentKind::LowConfidence
            };
            let note = match kind {
                SegmentKind::RecorderGap => format!(
                    "recorder produced no usable signal for {:.1} s here",
                    src_gap
                ),
                _ => format!(
                    "no confident match for {:.1} s of this span; the model is extrapolated",
                    src_gap
                ),
            };
            segments.push(Segment {
                kind,
                source_index: Some(source_index),
                source_start: Some(a.source_time),
                source_end: Some(b.source_time),
                reference_start: ref_start,
                reference_end: ref_end,
                confidence: Confidence::new(0.25, UncertaintyReason::LowPeakSeparation),
                note,
            });
        } else {
            segments.push(Segment {
                kind: SegmentKind::Matched,
                source_index: Some(source_index),
                source_start: Some(a.source_time),
                source_end: Some(b.source_time),
                reference_start: ref_start,
                reference_end: ref_end,
                confidence: Confidence::new(
                    (a.confidence.score + b.confidence.score) / 2.0,
                    UncertaintyReason::None,
                ),
                note: String::new(),
            });
        }
    }

    // Source material that lies beyond the end of the reference.
    let map_ref_end = map.reference_time(source_duration);
    if map_ref_end > reference_duration {
        let beyond_start = map
            .source_time(reference_duration)
            .max(0.0)
            .min(source_duration);
        segments.push(Segment {
            kind: SegmentKind::BeyondReference,
            source_index: Some(source_index),
            source_start: Some(beyond_start),
            source_end: Some(source_duration),
            reference_start: reference_duration,
            reference_end: map_ref_end,
            confidence: Confidence::new(0.0, UncertaintyReason::ReferenceSilent),
            note: format!(
                "{:.1} s of source material lies beyond the end of the reference; preserved in the source-aligned export only",
                map_ref_end - reference_duration
            ),
        });
    }

    // Reference material before the start of the source's coverage.
    let map_ref_start = map.reference_time(0.0);
    if map_ref_start > 0.0 {
        segments.push(Segment {
            kind: SegmentKind::UncoveredReference,
            source_index: None,
            source_start: None,
            source_end: None,
            reference_start: 0.0,
            reference_end: map_ref_start,
            confidence: Confidence::new(0.0, UncertaintyReason::LowEnergy),
            note: format!(
                "the reference starts {:.1} s before this source's coverage begins",
                map_ref_start
            ),
        });
    }

    segments.sort_by(|a, b| {
        a.reference_start
            .partial_cmp(&b.reference_start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Coverage statistics.
    let mut covered = 0.0;
    let mut low = 0.0;
    for s in &segments {
        match s.kind {
            SegmentKind::Matched => covered += s.duration(),
            SegmentKind::LowConfidence => low += s.duration(),
            _ => {}
        }
    }
    let uncovered = (reference_duration - covered - low).max(0.0);
    let coverage = Coverage {
        covered_seconds: covered,
        low_confidence_seconds: low,
        uncovered_seconds: uncovered,
        fraction: if reference_duration > 0.0 {
            (covered + low) / reference_duration
        } else {
            0.0
        },
    };

    (segments, coverage)
}

/// `true` when the source envelope is quiet throughout the interval.
fn source_silent_between(env: &Envelope, t0: f64, t1: f64) -> bool {
    let lo = env.index_at(t0);
    let hi = env.index_at(t1).min(env.len().saturating_sub(1));
    if hi <= lo {
        return false;
    }
    env.quiet[lo..=hi].iter().all(|q| *q)
}

/// The median spacing between consecutive trusted probes, in seconds.
///
/// This is the scale at which the matcher was actually working, so it is the right
/// reference for deciding whether a span between two probes is a genuine break.
fn median_probe_spacing(trusted: &[&MatchPoint], fallback: f64) -> f64 {
    if trusted.len() < 2 {
        return fallback.max(0.0);
    }
    let mut gaps: Vec<f64> = trusted
        .windows(2)
        .map(|w| (w[1].source_time - w[0].source_time).abs())
        .filter(|g| *g > 0.0)
        .collect();
    if gaps.is_empty() {
        return fallback.max(0.0);
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2]
}

/// The smallest gap that is treated as a genuine hole in the merged timeline.
///
/// Adjacent segments are computed from slightly different source positions, so their
/// reference boundaries differ by a few microseconds. Without a tolerance those
/// rounding differences would be reported as uncovered spans a few microseconds long.
const MERGE_GAP_EPSILON: f64 = 0.010;

/// Merge the per-source segments into one reference timeline.
///
/// Overlapping sources are resolved by source priority (the order the user supplied
/// them); the losing source's material is not deleted, it simply is not used in the
/// merged timeline.
pub fn merge_timeline(per_source: &[Vec<Segment>], reference_duration: f64) -> Vec<TimelineSpan> {
    // Collect the source-backed intervals.
    let mut covered: Vec<(f64, f64, usize, SegmentKind, Confidence)> = Vec::new();
    for (idx, segments) in per_source.iter().enumerate() {
        for s in segments {
            if matches!(s.kind, SegmentKind::Matched | SegmentKind::LowConfidence) {
                covered.push((
                    s.reference_start,
                    s.reference_end,
                    idx,
                    s.kind,
                    s.confidence,
                ));
            }
        }
    }
    covered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut spans: Vec<TimelineSpan> = Vec::new();
    let mut cursor = 0.0f64;

    for (start, end, idx, kind, confidence) in covered {
        let start = start.max(0.0);
        let end = end.min(reference_duration);
        if end <= start {
            continue;
        }
        if start > cursor + MERGE_GAP_EPSILON {
            spans.push(TimelineSpan {
                reference_start: cursor,
                reference_end: start,
                kind: SegmentKind::UncoveredReference,
                source_index: None,
                confidence: Confidence::new(0.0, UncertaintyReason::LowEnergy),
                note: "no recorder covered this span of the reference".into(),
            });
        }
        // Only the highest-priority source contributes at any point.
        if let Some(last) = spans.last_mut() {
            if last.source_index == Some(idx)
                && last.kind == kind
                && (last.reference_end - start).abs() < 1e-6
            {
                last.reference_end = end;
                cursor = end;
                continue;
            }
        }
        spans.push(TimelineSpan {
            reference_start: start,
            reference_end: end,
            kind,
            source_index: Some(idx),
            confidence,
            note: if kind == SegmentKind::LowConfidence {
                "covered, but the match here is not fully trusted".into()
            } else {
                String::new()
            },
        });
        cursor = end;
    }

    if cursor < reference_duration - MERGE_GAP_EPSILON {
        spans.push(TimelineSpan {
            reference_start: cursor,
            reference_end: reference_duration,
            kind: SegmentKind::UncoveredReference,
            source_index: None,
            confidence: Confidence::new(0.0, UncertaintyReason::LowEnergy),
            note: "no recorder covered this span of the reference".into(),
        });
    }

    spans
}
