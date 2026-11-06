//! The waveform timeline: the reference on top, the recordings below it.
//!
//! The picture is the alignment. A recording is drawn **where the sync engine put
//! it**, not where it was dropped: the reference's waveform runs across the top,
//! and each recording's waveform is drawn at the position its model says it
//! belongs. When the picture and the sound agree, the alignment is right; when
//! they do not, the misalignment is visible rather than merely reported.
//!
//! The waveforms come from the core's peak cache, which is built by streaming the
//! file. A three-hour recording is never held in memory: what is drawn here is a
//! few thousand numbers per lane.

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke};
use stagesync_core::audio::PeakCache;
use stagesync_core::matching::{LinearModel, SegmentKind};

/// The height of the reference lane, in points.
pub const REFERENCE_HEIGHT: f32 = 104.0;
/// The height of one recording lane, in points.
pub const TARGET_HEIGHT: f32 = 62.0;
/// The height of the caption above a lane, in points.
pub const CAPTION_HEIGHT: f32 = 15.0;
/// The vertical gap between two lanes, in points.
pub const LANE_GAP: f32 = 10.0;
/// The height of the coverage strip at the bottom of a lane, in points.
pub const COVERAGE_HEIGHT: f32 = 7.0;
/// The height of the time ruler under the lanes, in points.
pub const RULER_HEIGHT: f32 = 20.0;

/// One recording's lane.
pub struct TargetLane<'a> {
    /// The name shown above the lane.
    pub name: String,
    /// The peaks of the recording, when they have been built.
    pub peaks: Option<&'a PeakCache>,
    /// The model that places the recording on the reference timeline.
    pub model: Option<LinearModel>,
    /// The spans of the recording, with the kind of each.
    pub segments: Vec<(f64, f64, SegmentKind)>,
    /// Whether this recording is the one that is selected.
    pub selected: bool,
}

/// Everything the waveform timeline needs to draw itself.
pub struct WaveformView<'a> {
    /// The span of the reference timeline on screen, in seconds.
    pub start_secs: f64,
    /// The span of the reference timeline on screen, in seconds.
    pub end_secs: f64,
    /// The peaks of the reference recording.
    pub reference: Option<&'a PeakCache>,
    /// The merged coverage of the reference timeline.
    pub coverage: Vec<(f64, f64, SegmentKind)>,
    /// The recordings, one lane each.
    pub targets: Vec<TargetLane<'a>>,
    /// Where the playhead is, in seconds.
    pub playhead_secs: f64,
    /// The selected region, in seconds.
    pub selection: Option<(f64, f64)>,
    /// The region that is looped, in seconds.
    pub loop_region: Option<(f64, f64)>,
    /// The name of the recording that is playing, when something is.
    pub playing: Option<String>,
}

/// The total height the waveform timeline needs.
pub fn height(target_count: usize) -> f32 {
    CAPTION_HEIGHT
        + REFERENCE_HEIGHT
        + LANE_GAP
        + CAPTION_HEIGHT
        + target_count as f32 * (CAPTION_HEIGHT + TARGET_HEIGHT + LANE_GAP)
        + RULER_HEIGHT
}

/// The colour that stands for each kind of span.
pub fn kind_color(kind: SegmentKind) -> Color32 {
    match kind {
        SegmentKind::Matched => Color32::from_rgb(62, 158, 92),
        SegmentKind::LowConfidence => Color32::from_rgb(214, 168, 44),
        SegmentKind::UncoveredReference => Color32::from_rgb(198, 58, 58),
        SegmentKind::BeyondReference => Color32::from_gray(124),
        SegmentKind::RecorderGap => Color32::from_rgb(108, 20, 20),
    }
}

/// Draw the whole waveform timeline.
pub fn paint(painter: &Painter, rect: Rect, view: &WaveformView<'_>) {
    let span = (view.end_secs - view.start_secs).max(1e-6);
    let width = rect.width().max(1.0);
    let px_per_sec = width / span as f32;
    let x_of = |t: f64| rect.left() + ((t - view.start_secs) as f32) * px_per_sec;

    let mut y = rect.top();

    // The reference lane.
    paint_caption(painter, rect, y, "Reference".to_string());
    y += CAPTION_HEIGHT;
    paint_reference_lane(painter, rect, &x_of, y, view);
    y += REFERENCE_HEIGHT + LANE_GAP;

    // One lane per recording.
    for lane in &view.targets {
        let caption = if lane.selected {
            format!("{} · selected", lane.name)
        } else {
            lane.name.clone()
        };
        paint_caption(painter, rect, y, caption);
        y += CAPTION_HEIGHT;
        paint_target_lane(painter, rect, &x_of, y, lane, view);
        y += TARGET_HEIGHT + LANE_GAP;
    }

    paint_selection(
        painter,
        rect,
        &x_of,
        view.selection,
        Color32::from_rgb(90, 150, 230),
    );
    paint_loop(
        painter,
        rect,
        &x_of,
        view.loop_region,
        Color32::from_rgb(230, 150, 90),
    );
    paint_playhead(painter, rect, &x_of, view.playhead_secs);

    paint_ruler(
        painter,
        rect,
        &x_of,
        view.start_secs,
        view.end_secs,
        px_per_sec,
    );
}

/// Draw the caption of one lane.
fn paint_caption(painter: &Painter, rect: Rect, y: f32, text: String) {
    painter.text(
        Pos2::new(rect.left() + 2.0, y),
        Align2::LEFT_TOP,
        text,
        FontId::proportional(11.0),
        Color32::from_gray(170),
    );
}

/// Draw the reference's waveform and the coverage of the whole timeline.
fn paint_reference_lane(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    y: f32,
    view: &WaveformView<'_>,
) {
    let lane = Rect::from_min_max(
        Pos2::new(rect.left(), y),
        Pos2::new(rect.right(), y + REFERENCE_HEIGHT - COVERAGE_HEIGHT),
    );
    painter.rect_filled(lane, 2.0, Color32::from_gray(24));

    if let Some(cache) = view.reference {
        draw_waveform(
            painter,
            lane,
            cache,
            view.start_secs,
            view.end_secs,
            x_of,
            Color32::from_rgb(150, 170, 200),
        );
    }

    // The coverage strip: what the recordings between them cover, and what they
    // do not.
    let strip = Rect::from_min_max(
        Pos2::new(rect.left(), y + REFERENCE_HEIGHT - COVERAGE_HEIGHT),
        Pos2::new(rect.right(), y + REFERENCE_HEIGHT),
    );
    painter.rect_filled(strip, 0.0, Color32::from_gray(40));
    for (start, end, kind) in &view.coverage {
        let x0 = x_of(*start);
        let x1 = x_of(*end);
        if x1 < rect.left() || x0 > rect.right() {
            continue;
        }
        let span = Rect::from_min_max(
            Pos2::new(x0, strip.top()),
            Pos2::new(x1.max(x0 + 0.5), strip.bottom()),
        );
        painter.rect_filled(span, 0.0, kind_color(*kind));
    }
}

/// Draw one recording's waveform, at the position its model gives it.
fn paint_target_lane(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    y: f32,
    lane: &TargetLane<'_>,
    view: &WaveformView<'_>,
) {
    let area = Rect::from_min_max(
        Pos2::new(rect.left(), y),
        Pos2::new(rect.right(), y + TARGET_HEIGHT - COVERAGE_HEIGHT),
    );
    painter.rect_filled(area, 2.0, Color32::from_gray(24));

    if let (Some(cache), Some(model)) = (lane.peaks, lane.model) {
        // The part of the recording that belongs in the visible span of the
        // reference timeline.
        let source_start = (view.start_secs - model.offset) / model.scale;
        let source_end = (view.end_secs - model.offset) / model.scale;
        if source_end > 0.0 && source_start < cache.duration_secs() {
            let from = source_start.max(0.0);
            let to = source_end.min(cache.duration_secs());
            // The recording is drawn through the model, so the position of every
            // bucket is the reference time it corresponds to.
            draw_waveform(
                painter,
                area,
                cache,
                from,
                to,
                |t| x_of(model.reference_time(t)),
                Color32::from_rgb(120, 200, 140),
            );
        }
    }

    // The recording's own coverage.
    let strip = Rect::from_min_max(
        Pos2::new(rect.left(), y + TARGET_HEIGHT - COVERAGE_HEIGHT),
        Pos2::new(rect.right(), y + TARGET_HEIGHT),
    );
    painter.rect_filled(strip, 0.0, Color32::from_gray(40));
    for (start, end, kind) in &lane.segments {
        let x0 = x_of(*start);
        let x1 = x_of(*end);
        if x1 < rect.left() || x0 > rect.right() {
            continue;
        }
        let span = Rect::from_min_max(
            Pos2::new(x0, strip.top()),
            Pos2::new(x1.max(x0 + 0.5), strip.bottom()),
        );
        painter.rect_filled(span, 0.0, kind_color(*kind));
    }
}

/// Draw the peaks of a recording.
///
/// `place` turns a time in the recording into a position on screen. For the
/// reference it is the identity; for a recording that has been moved it is the
/// model, so the picture shows where the recording actually belongs.
fn draw_waveform(
    painter: &Painter,
    area: Rect,
    cache: &PeakCache,
    from_secs: f64,
    to_secs: f64,
    place: impl Fn(f64) -> f32,
    colour: Color32,
) {
    let buckets = area.width().ceil().max(1.0) as usize;
    let Some(window) = cache.peaks(from_secs, to_secs, buckets) else {
        return;
    };
    let (low, high) = window.extremes();
    let span = (high - low).max(1e-4);
    let middle = area.center().y;
    let half = area.height() * 0.5 - 1.0;
    let centre_value = (low + high) * 0.5;
    let scale = half / (span * 0.5);

    let stroke = Stroke::new(1.0, colour);
    for index in 0..window.buckets {
        let time = window.start_secs + (index as f64 + 0.5) * window.bucket_secs;
        let x = place(time);
        if x < area.left() - 1.0 || x > area.right() + 1.0 {
            continue;
        }
        let min = window
            .mins
            .iter()
            .filter_map(|plane| plane.get(index).copied())
            .fold(f32::INFINITY, f32::min);
        let max = window
            .maxs
            .iter()
            .filter_map(|plane| plane.get(index).copied())
            .fold(f32::NEG_INFINITY, f32::max);
        if !min.is_finite() || !max.is_finite() {
            continue;
        }
        let y0 = middle - (max - centre_value) * scale;
        let y1 = middle - (min - centre_value) * scale;
        painter.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], stroke);
    }
}

/// Draw the selected region.
fn paint_selection(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    selection: Option<(f64, f64)>,
    colour: Color32,
) {
    let Some((start, end)) = selection else {
        return;
    };
    let x0 = x_of(start);
    let x1 = x_of(end);
    let band = Rect::from_min_max(
        Pos2::new(x0, rect.top()),
        Pos2::new(x1.max(x0 + 0.5), rect.bottom()),
    );
    painter.rect_filled(band, 0.0, colour.gamma_multiply(0.16));
    painter.line_segment(
        [Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())],
        Stroke::new(1.0, colour),
    );
    painter.line_segment(
        [Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())],
        Stroke::new(1.0, colour),
    );
}

/// Draw the region that is looped.
fn paint_loop(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    loop_region: Option<(f64, f64)>,
    colour: Color32,
) {
    let Some((start, end)) = loop_region else {
        return;
    };
    let x0 = x_of(start);
    let x1 = x_of(end);
    let band = Rect::from_min_max(
        Pos2::new(x0, rect.top()),
        Pos2::new(x1.max(x0 + 0.5), rect.bottom()),
    );
    painter.rect_filled(band, 0.0, colour.gamma_multiply(0.12));
    painter.line_segment(
        [Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())],
        Stroke::new(2.0, colour),
    );
    painter.line_segment(
        [Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())],
        Stroke::new(2.0, colour),
    );
}

/// Draw the playhead.
fn paint_playhead(painter: &Painter, rect: Rect, x_of: &impl Fn(f64) -> f32, position: f64) {
    let x = x_of(position);
    painter.line_segment(
        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
        Stroke::new(1.5, Color32::from_rgb(240, 240, 240)),
    );
    painter.circle_filled(
        Pos2::new(x, rect.top() + 4.0),
        4.0,
        Color32::from_rgb(240, 240, 240),
    );
}

/// Draw the time ruler.
fn paint_ruler(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    start_secs: f64,
    end_secs: f64,
    px_per_sec: f32,
) {
    let y = rect.bottom() - RULER_HEIGHT;
    let step = tick_step(px_per_sec);
    let mut t = (start_secs / step).floor() * step;
    while t <= end_secs + step {
        let x = x_of(t);
        if x >= rect.left() - 2.0 && x <= rect.right() + 2.0 {
            painter.line_segment(
                [Pos2::new(x, y), Pos2::new(x, y + 5.0)],
                Stroke::new(1.0, Color32::from_gray(120)),
            );
            painter.text(
                Pos2::new(x + 2.0, y + 5.0),
                Align2::LEFT_TOP,
                format_tick(t),
                FontId::proportional(10.0),
                Color32::from_gray(150),
            );
        }
        t += step;
    }
}

/// The spacing between ruler ticks, chosen so the labels never collide.
fn tick_step(px_per_sec: f32) -> f64 {
    const NICE: [f64; 14] = [
        0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 300.0,
    ];
    for candidate in NICE {
        if candidate * px_per_sec as f64 >= 70.0 {
            return candidate;
        }
    }
    NICE[NICE.len() - 1]
}

/// A short label for a ruler tick.
fn format_tick(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else if minutes > 0 {
        format!("{minutes}:{secs:02}")
    } else {
        format!("{secs}s")
    }
}

/// The span a rectangle of the timeline covers, in seconds.
pub fn drag_span(rect: Rect, from_x: f32, to_x: f32, start_secs: f64, end_secs: f64) -> (f64, f64) {
    let span = (end_secs - start_secs).max(1e-6);
    let per_pixel = span / rect.width().max(1.0) as f64;
    let a = start_secs + (from_x - rect.left()) as f64 * per_pixel;
    let b = start_secs + (to_x - rect.left()) as f64 * per_pixel;
    (a.min(b), a.max(b))
}

/// The time a position on screen stands for.
pub fn time_at(rect: Rect, x: f32, start_secs: f64, end_secs: f64) -> f64 {
    let span = (end_secs - start_secs).max(1e-6);
    let per_pixel = span / rect.width().max(1.0) as f64;
    start_secs + (x - rect.left()) as f64 * per_pixel
}
