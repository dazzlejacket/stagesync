//! Custom painter for the lag and residual plots.
//!
//! No plotting library is used: both plots are scatter plots with one reference line,
//! and drawing them directly keeps the axes and their units explicit.

use eframe::egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use stagesync_core::matching::{LinearModel, MatchPoint};

/// Height of one plot, in points.
pub const PLOT_HEIGHT: f32 = 172.0;

const MARGIN_LEFT: f32 = 66.0;
const MARGIN_RIGHT: f32 = 14.0;
const MARGIN_TOP: f32 = 20.0;
const MARGIN_BOTTOM: f32 = 26.0;

/// Draw the measured lag against the source time, with the fitted model over it.
pub fn paint_lag_plot(
    ui: &mut Ui,
    points: &[MatchPoint],
    model: Option<LinearModel>,
    duration: f64,
) {
    let series: Vec<(f64, f64, f32)> = points
        .iter()
        .map(|p| (p.source_time, p.lag, p.confidence.score))
        .collect();
    let reference = model.map(|model| move |t: f64| model.reference_time(t) - t);
    paint_plot(
        ui,
        "measured lag vs source time",
        "lag (s)",
        &series,
        reference.as_ref().map(|f| f as &dyn Fn(f64) -> f64),
        duration,
        false,
    );
}

/// Draw the residual of every match point against the fitted model.
pub fn paint_residual_plot(ui: &mut Ui, points: &[MatchPoint], duration: f64) {
    let series: Vec<(f64, f64, f32)> = points
        .iter()
        .map(|p| (p.source_time, p.residual_ms, p.confidence.score))
        .collect();
    paint_plot(
        ui,
        "residual against the fitted model",
        "residual (ms)",
        &series,
        None,
        duration,
        true,
    );
}

/// Draw one scatter plot.
fn paint_plot(
    ui: &mut Ui,
    title: &str,
    y_label: &str,
    series: &[(f64, f64, f32)],
    reference_line: Option<&dyn Fn(f64) -> f64>,
    x_max: f64,
    symmetric: bool,
) {
    let width = ui.available_width().max(240.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, PLOT_HEIGHT), Sense::hover());
    let outer = response.rect;
    painter.rect_filled(outer, 3.0, Color32::from_gray(24));
    painter.text(
        Pos2::new(outer.left() + 6.0, outer.top() + 2.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(12.0),
        Color32::from_gray(195),
    );

    let plot = Rect::from_min_max(
        Pos2::new(outer.left() + MARGIN_LEFT, outer.top() + MARGIN_TOP),
        Pos2::new(outer.right() - MARGIN_RIGHT, outer.bottom() - MARGIN_BOTTOM),
    );
    if plot.width() <= 10.0 || plot.height() <= 10.0 {
        return;
    }

    let x_max = x_max.max(1.0);
    let (mut y_min, mut y_max) = value_range(series);
    if let Some(line) = reference_line {
        for t in [0.0, x_max] {
            let y = line(t);
            y_min = y_min.min(y);
            y_max = y_max.max(y);
        }
    }
    if symmetric {
        let extent = y_min.abs().max(y_max.abs()).max(1e-9);
        y_min = -extent;
        y_max = extent;
    }
    if (y_max - y_min).abs() < 1e-9 {
        y_min -= 1.0;
        y_max += 1.0;
    }
    let padding = (y_max - y_min) * 0.08;
    y_min -= padding;
    y_max += padding;
    if symmetric {
        let extent = y_min.abs().max(y_max.abs());
        y_min = -extent;
        y_max = extent;
    }

    let x_of = |t: f64| plot.left() + (t / x_max) as f32 * plot.width();
    let y_of = |y: f64| plot.bottom() - ((y - y_min) / (y_max - y_min)) as f32 * plot.height();

    painter.rect_stroke(
        plot,
        2.0,
        Stroke::new(1.0, Color32::from_gray(70)),
        StrokeKind::Middle,
    );

    if y_min < 0.0 && y_max > 0.0 {
        let zero = y_of(0.0);
        painter.line_segment(
            [Pos2::new(plot.left(), zero), Pos2::new(plot.right(), zero)],
            Stroke::new(1.0, Color32::from_gray(95)),
        );
    }

    if let Some(line) = reference_line {
        painter.line_segment(
            [
                Pos2::new(x_of(0.0), y_of(line(0.0))),
                Pos2::new(x_of(x_max), y_of(line(x_max))),
            ],
            Stroke::new(1.5, Color32::from_rgb(238, 196, 92)),
        );
    }

    for (t, y, score) in series {
        let point = Pos2::new(x_of(*t), y_of(*y));
        if point.x < plot.left() - 2.0 || point.x > plot.right() + 2.0 {
            continue;
        }
        let alpha = (60.0 + 195.0 * score.clamp(0.0, 1.0)) as u8;
        painter.circle_filled(
            point,
            3.0,
            Color32::from_rgba_unmultiplied(92, 172, 255, alpha),
        );
    }

    let label = || FontId::proportional(10.0);
    let grey = Color32::from_gray(150);
    painter.text(
        Pos2::new(plot.left() - 6.0, y_of(y_max)),
        Align2::RIGHT_TOP,
        format!("{y_max:.3}"),
        label(),
        grey,
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, y_of(y_min)),
        Align2::RIGHT_BOTTOM,
        format!("{y_min:.3}"),
        label(),
        grey,
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, plot.center().y),
        Align2::RIGHT_CENTER,
        y_label,
        label(),
        Color32::from_gray(175),
    );
    painter.text(
        Pos2::new(plot.left(), plot.bottom() + 3.0),
        Align2::LEFT_TOP,
        "0 s",
        label(),
        grey,
    );
    painter.text(
        Pos2::new(plot.right(), plot.bottom() + 3.0),
        Align2::RIGHT_TOP,
        format!("{x_max:.1} s"),
        label(),
        grey,
    );
}

/// The lowest and highest value of a series.
fn value_range(series: &[(f64, f64, f32)]) -> (f64, f64) {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (_, y, _) in series {
        low = low.min(*y);
        high = high.max(*y);
    }
    if !low.is_finite() || !high.is_finite() {
        return (-1.0, 1.0);
    }
    (low, high)
}
