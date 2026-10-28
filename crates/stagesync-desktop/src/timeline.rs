//! Painting of the reference timeline.
//!
//! The timeline is the honest part of the application: a span of the reference that no
//! recorder captured is drawn as an empty, hatched band, never as continuous material.

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, Vec2};
use stagesync_core::matching::{AlignmentReport, SegmentKind};

/// Height of one lane, in points.
pub const LANE_HEIGHT: f32 = 24.0;
/// Height of the caption above one lane, in points.
pub const CAPTION_HEIGHT: f32 = 15.0;
/// Vertical gap between two lanes, in points.
pub const LANE_GAP: f32 = 6.0;
/// Height of the time ruler under the lanes, in points.
pub const RULER_HEIGHT: f32 = 22.0;

/// The colour that stands for each kind of span.
pub fn kind_color(kind: SegmentKind) -> Color32 {
    match kind {
        SegmentKind::Matched => Color32::from_rgb(62, 158, 92),
        SegmentKind::LowConfidence => Color32::from_rgb(214, 168, 44),
        SegmentKind::UncoveredReference => Color32::from_rgb(198, 58, 58),
        SegmentKind::BeyondReference => Color32::from_rgb(124, 124, 130),
        SegmentKind::RecorderGap => Color32::from_rgb(108, 20, 20),
    }
}

/// The part of a span the painter needs.
struct Span {
    reference_start: f64,
    reference_end: f64,
    kind: SegmentKind,
    /// The recorder that owns the span, when there is one.
    owner: Option<String>,
}

/// The total height the timeline needs for a report with this many sources.
pub fn timeline_height(source_count: usize) -> f32 {
    let lanes = source_count + 1;
    CAPTION_HEIGHT + (lanes as f32) * (CAPTION_HEIGHT + LANE_HEIGHT + LANE_GAP) + RULER_HEIGHT
}

/// Draw the merged timeline and one lane per source.
///
/// `px_per_sec` is the zoom: the number of points one second of the reference takes.
pub fn paint_timeline(painter: &Painter, rect: Rect, report: &AlignmentReport, px_per_sec: f32) {
    let duration = report.timeline_duration.max(0.0);
    let scale = px_per_sec.max(0.01);
    let viewport = painter.clip_rect();
    let x_of = |t: f64| rect.left() + (t as f32) * scale;

    let mut y = rect.top();

    paint_caption(
        painter,
        viewport,
        y,
        format!(
            "{:.1} s of reference timeline · merged view, then one lane per recorder",
            duration
        ),
    );
    y += CAPTION_HEIGHT;

    let merged: Vec<Span> = report
        .timeline
        .iter()
        .map(|span| Span {
            reference_start: span.reference_start,
            reference_end: span.reference_end,
            kind: span.kind,
            owner: span.source_index.and_then(|index| {
                report
                    .sources
                    .get(index)
                    .map(|source| source.source.file_name.clone())
            }),
        })
        .collect();
    paint_lane(painter, viewport, &x_of, y, "merged", &merged);
    y += CAPTION_HEIGHT + LANE_HEIGHT + LANE_GAP;

    for alignment in &report.sources {
        let owner = alignment.source.file_name.clone();
        let spans: Vec<Span> = alignment
            .segments
            .iter()
            .map(|span| Span {
                reference_start: span.reference_start,
                reference_end: span.reference_end,
                kind: span.kind,
                owner: Some(owner.clone()),
            })
            .collect();
        paint_lane(
            painter,
            viewport,
            &x_of,
            y,
            &format!("recorder: {owner}"),
            &spans,
        );
        y += CAPTION_HEIGHT + LANE_HEIGHT + LANE_GAP;
    }

    paint_ruler(painter, viewport, &x_of, y, duration, scale);
}

/// Draw the caption of one lane at the left edge of the visible area.
fn paint_caption(painter: &Painter, viewport: Rect, y: f32, text: String) {
    painter.text(
        Pos2::new(viewport.left() + 2.0, y),
        Align2::LEFT_TOP,
        text,
        FontId::proportional(11.0),
        Color32::from_gray(170),
    );
}

/// Draw one lane of spans.
fn paint_lane(
    painter: &Painter,
    viewport: Rect,
    x_of: &impl Fn(f64) -> f32,
    y: f32,
    caption: &str,
    spans: &[Span],
) {
    paint_caption(painter, viewport, y, caption.to_string());

    let lane_top = y + CAPTION_HEIGHT;
    let lane_rect = Rect::from_min_max(
        Pos2::new(viewport.left(), lane_top),
        Pos2::new(viewport.right(), lane_top + LANE_HEIGHT),
    );
    painter.rect_filled(lane_rect, 2.0, Color32::from_gray(26));

    for span in spans {
        let x0 = x_of(span.reference_start);
        let x1 = x_of(span.reference_end);
        if x1 < viewport.left() - 1.0 || x0 > viewport.right() + 1.0 {
            continue;
        }
        let left = x0.min(x1);
        let right = x0.max(x1).max(left + 0.5);
        let rect = Rect::from_min_max(
            Pos2::new(left, lane_top),
            Pos2::new(right, lane_top + LANE_HEIGHT),
        );

        painter.rect_filled(rect, 2.0, kind_color(span.kind));
        if matches!(
            span.kind,
            SegmentKind::UncoveredReference | SegmentKind::RecorderGap
        ) {
            // Hatching marks the band as missing material rather than as audio.
            hatch(painter, rect, Color32::from_black_alpha(130), 6.0);
        }

        if rect.width() > 48.0 {
            let owner = span.owner.as_deref().unwrap_or("no recorder");
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("{} · {owner}", span.kind.label()),
                FontId::proportional(11.0),
                Color32::from_rgb(238, 238, 238),
            );
        }
    }
}

/// Draw the time ruler under the lanes.
fn paint_ruler(
    painter: &Painter,
    viewport: Rect,
    x_of: &impl Fn(f64) -> f32,
    y: f32,
    duration: f64,
    scale: f32,
) {
    let step = tick_step(scale);
    let mut t = 0.0;
    while t <= duration + 1e-6 {
        let x = x_of(t);
        if x >= viewport.left() - 2.0 && x <= viewport.right() + 2.0 {
            painter.line_segment(
                [Pos2::new(x, y), Pos2::new(x, y + 5.0)],
                Stroke::new(1.0, Color32::from_gray(120)),
            );
            painter.text(
                Pos2::new(x + 2.0, y + 6.0),
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
fn tick_step(scale: f32) -> f64 {
    const NICE: [f64; 13] = [
        0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0,
    ];
    for candidate in NICE {
        if candidate * scale as f64 >= 70.0 {
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
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// Fill a rectangle with diagonal lines.
fn hatch(painter: &Painter, rect: Rect, colour: Color32, spacing: f32) {
    let height = rect.height();
    if height <= 0.0 || rect.width() <= 0.0 {
        return;
    }
    let stroke = Stroke::new(1.0, colour);
    let mut x = rect.left() - height;
    while x < rect.right() {
        let x0 = x.max(rect.left());
        let x1 = (x + height).min(rect.right());
        if x1 > x0 {
            let t0 = (x0 - x) / height;
            let t1 = (x1 - x) / height;
            let y0 = rect.bottom() - t0 * height;
            let y1 = rect.bottom() - t1 * height;
            painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y1)], stroke);
        }
        x += spacing;
    }
}

/// Draw the colour legend.
pub fn paint_legend(ui: &mut eframe::egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for kind in [
            SegmentKind::Matched,
            SegmentKind::LowConfidence,
            SegmentKind::UncoveredReference,
            SegmentKind::RecorderGap,
            SegmentKind::BeyondReference,
        ] {
            let (response, painter) = ui.allocate_painter(Vec2::new(13.0, 13.0), Sense::hover());
            painter.rect_filled(response.rect, 2.0, kind_color(kind));
            if matches!(
                kind,
                SegmentKind::UncoveredReference | SegmentKind::RecorderGap
            ) {
                hatch(&painter, response.rect, Color32::from_black_alpha(130), 5.0);
            }
            ui.label(kind.label());
        }
    });
}
