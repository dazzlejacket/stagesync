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

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind};
use stagesync_core::audio::PeakCache;
use stagesync_core::matching::{LinearModel, SegmentKind};

use crate::theme::Palette;

/// The height of the overview minimap bar, in points.
pub const OVERVIEW_HEIGHT: f32 = 22.0;
/// The height of the reference lane, in points.
pub const REFERENCE_HEIGHT: f32 = 112.0;
/// The height of one recording lane, in points.
pub const TARGET_HEIGHT: f32 = 74.0;
/// The height of the caption above a lane, in points.
pub const CAPTION_HEIGHT: f32 = 16.0;
/// The vertical gap between two lanes, in points.
pub const LANE_GAP: f32 = 8.0;
/// The height of the coverage strip at the bottom of a lane, in points.
pub const COVERAGE_HEIGHT: f32 = 6.0;
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
    /// Pending un-applied nudge in seconds, highlighted in lane caption when non-zero.
    pub pending_nudge_secs: f64,
    /// Whether manual anchor edits await clicking "Apply anchor".
    pub unapplied_anchor: bool,
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
pub fn kind_color(kind: SegmentKind, palette: &Palette) -> Color32 {
    match kind {
        SegmentKind::Matched => palette.ok,
        SegmentKind::LowConfidence => palette.warn,
        SegmentKind::UncoveredReference => palette.error,
        SegmentKind::BeyondReference => palette.beyond,
        SegmentKind::RecorderGap => palette.gap_dark,
    }
}

/// Draw the full-show overview minimap showing coverage and current zoom bracket.
pub fn paint_overview(
    painter: &Painter,
    rect: Rect,
    view: &WaveformView<'_>,
    total_duration: f64,
    palette: &Palette,
) {
    let dur = total_duration.max(1.0);
    painter.rect_filled(rect, 3.0, palette.bg_lane);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, palette.border_subtle),
        StrokeKind::Middle,
    );

    let x_of = |t: f64| rect.left() + ((t / dur) as f32).clamp(0.0, 1.0) * rect.width();

    if let Some(cache) = view.reference {
        let wave_rect = rect.shrink2(eframe::egui::vec2(1.0, 3.0));
        draw_waveform(
            painter,
            wave_rect,
            cache,
            0.0,
            dur,
            x_of,
            palette.wave_ref.gamma_multiply(0.55),
            palette.grid_line,
        );
    }

    // Coverage indicator bar along bottom 4px of overview.
    let strip = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.bottom() - 4.0),
        Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0),
    );
    for (start, end, kind) in &view.coverage {
        let x0 = x_of(*start);
        let x1 = x_of(*end).max(x0 + 1.0);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x0, strip.top()), Pos2::new(x1, strip.bottom())),
            0.0,
            kind_color(*kind, palette),
        );
    }

    // Current viewport bracket.
    let vx0 = x_of(view.start_secs).max(rect.left());
    let vx1 = x_of(view.end_secs).min(rect.right()).max(vx0 + 4.0);
    let bracket = Rect::from_min_max(
        Pos2::new(vx0, rect.top() + 1.0),
        Pos2::new(vx1, rect.bottom() - 1.0),
    );
    painter.rect_filled(bracket, 2.0, palette.accent_soft);
    painter.rect_stroke(
        bracket,
        2.0,
        Stroke::new(1.2, palette.accent),
        StrokeKind::Middle,
    );

    // Playhead tick on overview.
    let px = x_of(view.playhead_secs);
    painter.line_segment(
        [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
        Stroke::new(1.5, palette.playhead),
    );
}

/// Draw the whole waveform timeline.
pub fn paint(painter: &Painter, rect: Rect, view: &WaveformView<'_>, palette: &Palette) {
    let span = (view.end_secs - view.start_secs).max(1e-6);
    let width = rect.width().max(1.0);
    let px_per_sec = width / span as f32;
    let x_of = |t: f64| rect.left() + ((t - view.start_secs) as f32) * px_per_sec;

    let mut y = rect.top();

    // The reference lane.
    let ref_caption = match view.reference {
        Some(cache) if cache.channels >= 2 => {
            format!(
                "Reference  ·  Stereo ({} ch, {:.1} s)",
                cache.channels,
                cache.duration_secs()
            )
        }
        Some(cache) => format!("Reference  ·  Mono ({:.1} s)", cache.duration_secs()),
        None => "Reference".to_string(),
    };
    paint_caption(painter, rect, y, ref_caption, false, palette);
    y += CAPTION_HEIGHT;
    paint_reference_lane(painter, rect, &x_of, y, view, palette);
    y += REFERENCE_HEIGHT + LANE_GAP;

    // One lane per recording.
    for lane in &view.targets {
        let ch_note = lane
            .peaks
            .map(|c| if c.channels >= 2 { " · L/R" } else { "" })
            .unwrap_or("");
        let mut caption = if lane.selected {
            format!("{}{ch_note} · selected", lane.name)
        } else {
            format!("{}{ch_note}", lane.name)
        };
        if lane.pending_nudge_secs.abs() > 1e-9 {
            caption.push_str(&format!(
                "  [PENDING NUDGE {:+.1} ms]",
                lane.pending_nudge_secs * 1000.0
            ));
        }
        if lane.unapplied_anchor {
            caption.push_str("  [UNAPPLIED ANCHOR]");
        }
        paint_caption(painter, rect, y, caption, lane.selected, palette);
        y += CAPTION_HEIGHT;
        paint_target_lane(painter, rect, &x_of, y, lane, view, palette);
        y += TARGET_HEIGHT + LANE_GAP;
    }

    paint_selection(painter, rect, &x_of, view.selection, palette.accent);
    paint_loop(painter, rect, &x_of, view.loop_region, palette.loop_stroke);
    paint_playhead(painter, rect, &x_of, view.playhead_secs, palette);

    paint_ruler(
        painter,
        rect,
        &x_of,
        view.start_secs,
        view.end_secs,
        px_per_sec,
        palette,
    );
}

/// Draw the caption of one lane.
fn paint_caption(
    painter: &Painter,
    rect: Rect,
    y: f32,
    text: String,
    selected: bool,
    palette: &Palette,
) {
    painter.text(
        Pos2::new(rect.left() + 2.0, y),
        Align2::LEFT_TOP,
        text,
        FontId::proportional(11.0),
        if selected {
            palette.accent
        } else {
            palette.text_muted
        },
    );
}

/// Draw the reference's waveform and the coverage of the whole timeline.
fn paint_reference_lane(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    y: f32,
    view: &WaveformView<'_>,
    palette: &Palette,
) {
    let lane = Rect::from_min_max(
        Pos2::new(rect.left(), y),
        Pos2::new(rect.right(), y + REFERENCE_HEIGHT - COVERAGE_HEIGHT),
    );
    painter.rect_filled(lane, 2.0, palette.bg_lane);
    painter.rect_stroke(
        lane,
        2.0,
        Stroke::new(1.0, palette.border_subtle),
        StrokeKind::Middle,
    );

    if let Some(cache) = view.reference {
        draw_waveform(
            painter,
            lane,
            cache,
            view.start_secs,
            view.end_secs,
            x_of,
            palette.wave_ref,
            palette.grid_line,
        );
    }

    // The coverage strip: what the recordings between them cover, and what they do not.
    let strip = Rect::from_min_max(
        Pos2::new(rect.left(), y + REFERENCE_HEIGHT - COVERAGE_HEIGHT),
        Pos2::new(rect.right(), y + REFERENCE_HEIGHT),
    );
    painter.rect_filled(strip, 0.0, palette.bg_strip);
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
        painter.rect_filled(span, 0.0, kind_color(*kind, palette));
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
    palette: &Palette,
) {
    let area = Rect::from_min_max(
        Pos2::new(rect.left(), y),
        Pos2::new(rect.right(), y + TARGET_HEIGHT - COVERAGE_HEIGHT),
    );
    painter.rect_filled(
        area,
        2.0,
        if lane.selected {
            palette.bg_lane_selected
        } else {
            palette.bg_lane
        },
    );
    painter.rect_stroke(
        area,
        2.0,
        Stroke::new(
            1.0,
            if lane.selected {
                palette.accent
            } else {
                palette.border_subtle
            },
        ),
        StrokeKind::Middle,
    );

    if let (Some(cache), Some(model)) = (lane.peaks, lane.model) {
        let source_start = (view.start_secs - model.offset) / model.scale;
        let source_end = (view.end_secs - model.offset) / model.scale;
        if source_end > 0.0 && source_start < cache.duration_secs() {
            let from = source_start.max(0.0);
            let to = source_end.min(cache.duration_secs());
            draw_waveform(
                painter,
                area,
                cache,
                from,
                to,
                |t| x_of(model.reference_time(t)),
                palette.wave_target,
                palette.grid_line,
            );
        }
    } else if let Some(cache) = lane.peaks {
        // Before alignment finishes, still render the unaligned preview waveform faintly.
        let to = view.end_secs.min(cache.duration_secs()).max(0.0);
        let from = view.start_secs.clamp(0.0, to);
        if to > from {
            draw_waveform(
                painter,
                area,
                cache,
                from,
                to,
                x_of,
                palette.text_faint,
                palette.grid_line,
            );
        }
    }

    // The recording's own coverage.
    let strip = Rect::from_min_max(
        Pos2::new(rect.left(), y + TARGET_HEIGHT - COVERAGE_HEIGHT),
        Pos2::new(rect.right(), y + TARGET_HEIGHT),
    );
    painter.rect_filled(strip, 0.0, palette.bg_strip);
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
        painter.rect_filled(span, 0.0, kind_color(*kind, palette));
    }
}

/// Draw the peaks of a recording (split L/R sub-lanes for stereo recordings).
#[allow(clippy::too_many_arguments)]
fn draw_waveform(
    painter: &Painter,
    area: Rect,
    cache: &PeakCache,
    from_secs: f64,
    to_secs: f64,
    place: impl Fn(f64) -> f32,
    colour: Color32,
    grid_line: Color32,
) {
    let buckets = area.width().ceil().max(1.0) as usize;
    let Some(window) = cache.peaks(from_secs, to_secs, buckets) else {
        return;
    };

    if window.mins.len() >= 2 && window.maxs.len() >= 2 {
        // Stereo L/R split waveform display.
        let mid_y = area.center().y;
        painter.line_segment(
            [
                Pos2::new(area.left(), mid_y),
                Pos2::new(area.right(), mid_y),
            ],
            Stroke::new(1.0, grid_line),
        );
        let top_rect = Rect::from_min_max(area.left_top(), Pos2::new(area.right(), mid_y - 1.0));
        let bot_rect = Rect::from_min_max(Pos2::new(area.left(), mid_y + 1.0), area.right_bottom());
        draw_single_channel(painter, top_rect, &window, 0, &place, colour, grid_line);
        draw_single_channel(painter, bot_rect, &window, 1, &place, colour, grid_line);
    } else {
        draw_single_channel(painter, area, &window, 0, &place, colour, grid_line);
    }
}

fn draw_single_channel(
    painter: &Painter,
    area: Rect,
    window: &stagesync_core::audio::PeakWindow,
    channel: usize,
    place: &impl Fn(f64) -> f32,
    colour: Color32,
    grid_line: Color32,
) {
    let Some(mins) = window.mins.get(channel) else {
        return;
    };
    let Some(maxs) = window.maxs.get(channel) else {
        return;
    };
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for (&mn, &mx) in mins.iter().zip(maxs.iter()) {
        if mn.is_finite() {
            low = low.min(mn);
        }
        if mx.is_finite() {
            high = high.max(mx);
        }
    }
    if !low.is_finite() || !high.is_finite() {
        return;
    }

    let middle = area.center().y;
    painter.line_segment(
        [
            Pos2::new(area.left(), middle),
            Pos2::new(area.right(), middle),
        ],
        Stroke::new(1.0, grid_line.gamma_multiply(0.5)),
    );

    let span = (high - low).max(1e-4);
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
        let min = mins.get(index).copied().unwrap_or(0.0);
        let max = maxs.get(index).copied().unwrap_or(0.0);
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
fn paint_playhead(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    position: f64,
    palette: &Palette,
) {
    let x = x_of(position);
    if x < rect.left() - 2.0 || x > rect.right() + 2.0 {
        return;
    }
    painter.line_segment(
        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
        Stroke::new(1.5, palette.playhead),
    );
    let head = vec![
        Pos2::new(x - 4.5, rect.top()),
        Pos2::new(x + 4.5, rect.top()),
        Pos2::new(x, rect.top() + 7.0),
    ];
    painter.add(eframe::egui::Shape::convex_polygon(
        head,
        palette.playhead,
        Stroke::NONE,
    ));
}

/// Draw the time ruler.
fn paint_ruler(
    painter: &Painter,
    rect: Rect,
    x_of: &impl Fn(f64) -> f32,
    start_secs: f64,
    end_secs: f64,
    px_per_sec: f32,
    palette: &Palette,
) {
    let y = rect.bottom() - RULER_HEIGHT;
    painter.line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0, palette.border_subtle),
    );
    let step = tick_step(px_per_sec);
    let mut t = (start_secs / step).floor() * step;
    while t <= end_secs + step {
        let x = x_of(t);
        if x >= rect.left() - 2.0 && x <= rect.right() + 2.0 {
            painter.line_segment(
                [Pos2::new(x, y), Pos2::new(x, y + 5.0)],
                Stroke::new(1.0, palette.ruler_tick),
            );
            painter.text(
                Pos2::new(x + 2.0, y + 5.0),
                Align2::LEFT_TOP,
                format_tick(t),
                FontId::proportional(10.0),
                palette.text_muted,
            );
        }
        t += step;
    }
}

/// Public wrapper for ruler interval spacing given visible seconds and lane width in pixels.
pub fn ruler_tick_step(view_secs: f64, width_px: f32) -> f64 {
    let px_per_sec = width_px.max(1.0) / view_secs.max(1e-6) as f32;
    tick_step(px_per_sec)
}

/// The spacing between ruler ticks, chosen so multi-hour concert labels never collide.
fn tick_step(px_per_sec: f32) -> f64 {
    const NICE: [f64; 21] = [
        0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0,
        600.0, 900.0, 1800.0, 3600.0, 7200.0, 14400.0,
    ];
    const MIN_LABEL_PX: f64 = 84.0;
    let pps = (px_per_sec as f64).max(1e-6);
    for candidate in NICE {
        if candidate * pps >= MIN_LABEL_PX {
            return candidate;
        }
    }
    let mut step = NICE[NICE.len() - 1];
    while step * pps < MIN_LABEL_PX {
        step *= 2.0;
    }
    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concert_ruler_labels_keep_minimum_pixel_separation() {
        for (duration, width) in [(9_685.0, 420.0f32), (9_685.0, 800.0), (86_400.0, 300.0)] {
            let step = ruler_tick_step(duration, width);
            let px_gap = step * (width as f64 / duration);
            assert!(
                px_gap >= 84.0,
                "ruler ticks collapsed to {px_gap:.1} px at {duration}s / {width}px (step={step}s)"
            );
        }
    }
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
