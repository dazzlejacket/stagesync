//! Custom painter for the lag, residual and drift plots.
//!
//! No plotting library is used: the plots are scatter plots with one reference line,
//! and drawing them directly keeps the axes and their units explicit.

use eframe::egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use stagesync_core::matching::{LinearModel, MatchMethod, MatchPoint};

use crate::theme::Palette;

/// Height of one plot, in points.
pub const PLOT_HEIGHT: f32 = 172.0;

const MARGIN_LEFT: f32 = 66.0;
const MARGIN_RIGHT: f32 = 14.0;
const MARGIN_TOP: f32 = 20.0;
const MARGIN_BOTTOM: f32 = 26.0;

/// How a series is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// A filled circle.
    Circle,
    /// A filled square.
    Square,
    /// A hollow square: the mark of a measurement that did not come from the fit.
    HollowSquare,
}

/// One set of points on a plot.
#[derive(Debug, Clone)]
pub struct Series {
    /// What the series is, shown in the legend.
    pub label: &'static str,
    /// The colour of the points.
    pub colour: Color32,
    /// How the points are drawn.
    pub marker: Marker,
    /// The points: source time, value, confidence.
    pub points: Vec<(f64, f64, f32)>,
}

/// What one plot shows.
struct Plot<'a> {
    /// The title, at the top of the plot.
    title: &'a str,
    /// The label of the vertical axis, with its unit.
    y_label: &'a str,
    /// The series that are drawn.
    series: &'a [Series],
    /// A line drawn over the series, when there is one.
    reference_line: Option<&'a dyn Fn(f64) -> f64>,
    /// The right-hand end of the horizontal axis, in seconds.
    x_max: f64,
    /// Whether the vertical axis is symmetric around zero.
    symmetric: bool,
    /// Whether the legend is drawn.
    legend: bool,
    /// Active theme palette.
    palette: &'a Palette,
}

/// Draw the measured lag against the source time, with the fitted model over it.
pub fn paint_lag_plot(
    ui: &mut Ui,
    points: &[MatchPoint],
    model: Option<LinearModel>,
    duration: f64,
    palette: &Palette,
) {
    let series = Series {
        label: "measured lag",
        colour: palette.wave_ref,
        marker: Marker::Circle,
        points: points
            .iter()
            .map(|p| (p.source_time, p.lag, p.confidence.score))
            .collect(),
    };
    let reference = model.map(|model| move |t: f64| model.reference_time(t) - t);
    paint_plot(
        ui,
        Plot {
            title: "measured lag vs source time",
            y_label: "lag (s)",
            series: &[series],
            reference_line: reference.as_ref().map(|f| f as &dyn Fn(f64) -> f64),
            x_max: duration,
            symmetric: false,
            legend: false,
            palette,
        },
    );
}

/// Draw the residual of every match point against the fitted model.
pub fn paint_residual_plot(ui: &mut Ui, points: &[MatchPoint], duration: f64, palette: &Palette) {
    let series = Series {
        label: "residual",
        colour: palette.wave_ref,
        marker: Marker::Circle,
        points: points
            .iter()
            .map(|p| (p.source_time, p.residual_ms, p.confidence.score))
            .collect(),
    };
    paint_plot(
        ui,
        Plot {
            title: "residual against the fitted model",
            y_label: "residual (ms)",
            series: &[series],
            reference_line: None,
            x_max: duration,
            symmetric: true,
            legend: false,
            palette,
        },
    );
}

/// Draw the drift measured at every match point, relative to the fitted model.
pub fn paint_drift_plot(
    ui: &mut Ui,
    points: &[MatchPoint],
    model: Option<LinearModel>,
    duration: f64,
    palette: &Palette,
) {
    let Some(model) = model else {
        ui.label("There is no model, so there is nothing to compare the measurements against.");
        return;
    };

    let mut envelope = Vec::new();
    let mut waveform = Vec::new();
    for point in points {
        let Some(ppm) = drift_ppm_at(point, Some(model)) else {
            continue;
        };
        let entry = (point.source_time, ppm, point.confidence.score);
        if point.method == MatchMethod::WaveformPhat {
            waveform.push(entry);
        } else {
            envelope.push(entry);
        }
    }

    let series = [
        Series {
            label: "envelope",
            colour: palette.wave_ref,
            marker: Marker::Circle,
            points: envelope,
        },
        Series {
            label: "waveform (gcc_phat)",
            colour: palette.wave_series_alt,
            marker: Marker::HollowSquare,
            points: waveform,
        },
    ];
    paint_plot(
        ui,
        Plot {
            title: "drift against the model, by source time",
            y_label: "drift (ppm)",
            series: &series,
            reference_line: None,
            x_max: duration,
            symmetric: true,
            legend: true,
            palette,
        },
    );
}

/// The local drift a measurement implies, relative to the model, in parts per million.
///
/// `None` when the measurement is too close to the start of the recording to say
/// anything: the divisor is the source time, so a measurement one second into the
/// file turns a millisecond of noise into a thousand parts per million.
pub fn drift_ppm_at(point: &MatchPoint, model: Option<LinearModel>) -> Option<f64> {
    /// The earliest source time that can carry a usable rate.
    const MIN_SOURCE_TIME: f64 = 1.0;

    if !point.source_time.is_finite() || point.source_time < MIN_SOURCE_TIME {
        return None;
    }
    let measured_lag = point.reference_time - point.source_time;
    let model_lag = model
        .map(|model| model.reference_time(point.source_time) - point.source_time)
        .unwrap_or(0.0);
    Some((measured_lag - model_lag) / point.source_time * 1_000_000.0)
}

/// Draw one scatter plot.
fn paint_plot(ui: &mut Ui, plot: Plot<'_>) {
    let Plot {
        title,
        y_label,
        series,
        reference_line,
        x_max,
        symmetric,
        legend,
        palette,
    } = plot;
    let width = ui.available_width().max(240.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, PLOT_HEIGHT), Sense::hover());
    let outer = response.rect;
    painter.rect_filled(outer, 3.0, palette.bg_lane);
    painter.rect_stroke(
        outer,
        3.0,
        Stroke::new(1.0, palette.border_subtle),
        StrokeKind::Middle,
    );
    painter.text(
        Pos2::new(outer.left() + 8.0, outer.top() + 3.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(11.5),
        palette.text_primary,
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
        Stroke::new(1.0, palette.border_subtle),
        StrokeKind::Middle,
    );

    if y_min < 0.0 && y_max > 0.0 {
        let zero = y_of(0.0);
        painter.line_segment(
            [Pos2::new(plot.left(), zero), Pos2::new(plot.right(), zero)],
            Stroke::new(1.0, palette.grid_line),
        );
    }

    if let Some(line) = reference_line {
        painter.line_segment(
            [
                Pos2::new(x_of(0.0), y_of(line(0.0))),
                Pos2::new(x_of(x_max), y_of(line(x_max))),
            ],
            Stroke::new(1.5, palette.wave_series_alt),
        );
    }

    for series in series {
        for (t, y, score) in &series.points {
            let point = Pos2::new(x_of(*t), y_of(*y));
            if point.x < plot.left() - 2.0 || point.x > plot.right() + 2.0 {
                continue;
            }
            let alpha = (60.0 + 195.0 * score.clamp(0.0, 1.0)) as u8;
            paint_marker(&painter, point, series.marker, series.colour, alpha);
        }
    }

    if legend {
        let width = legend_width(series);
        paint_legend(
            &painter,
            series,
            Pos2::new(outer.right() - MARGIN_RIGHT - width, outer.top() + 3.0),
            palette,
        );
    }

    let label = || FontId::proportional(10.0);
    let tick_col = palette.text_muted;
    painter.text(
        Pos2::new(plot.left() - 6.0, y_of(y_max)),
        Align2::RIGHT_TOP,
        format!("{y_max:.3}"),
        label(),
        tick_col,
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, y_of(y_min)),
        Align2::RIGHT_BOTTOM,
        format!("{y_min:.3}"),
        label(),
        tick_col,
    );
    painter.text(
        Pos2::new(plot.left() - 6.0, plot.center().y),
        Align2::RIGHT_CENTER,
        y_label,
        label(),
        palette.text_primary,
    );
    painter.text(
        Pos2::new(plot.left(), plot.bottom() + 3.0),
        Align2::LEFT_TOP,
        "0 s",
        label(),
        tick_col,
    );
    painter.text(
        Pos2::new(plot.right(), plot.bottom() + 3.0),
        Align2::RIGHT_TOP,
        format!("{x_max:.1} s"),
        label(),
        tick_col,
    );
}

/// Draw one measurement.
fn paint_marker(
    painter: &eframe::egui::Painter,
    at: Pos2,
    marker: Marker,
    colour: Color32,
    alpha: u8,
) {
    let fill = Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha);
    match marker {
        Marker::Circle => {
            painter.circle_filled(at, 3.0, fill);
        }
        Marker::Square => {
            painter.rect_filled(Rect::from_center_size(at, Vec2::splat(6.0)), 1.0, fill);
        }
        Marker::HollowSquare => {
            let rect = Rect::from_center_size(at, Vec2::splat(7.0));
            painter.rect_filled(
                rect,
                1.0,
                Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha / 4),
            );
            painter.rect_stroke(rect, 1.0, Stroke::new(1.5, fill), StrokeKind::Middle);
        }
    }
}

/// The width the legend of a plot needs.
fn legend_width(series: &[Series]) -> f32 {
    series
        .iter()
        .map(|series| 11.0 + 6.5 * series.label.chars().count() as f32 + 14.0)
        .sum()
}

/// Draw the legend of a plot, at the top of it.
fn paint_legend(painter: &eframe::egui::Painter, series: &[Series], at: Pos2, palette: &Palette) {
    let font = FontId::proportional(10.0);
    let mut x = at.x;
    for series in series {
        paint_marker(
            painter,
            Pos2::new(x, at.y + 6.0),
            series.marker,
            series.colour,
            255,
        );
        let text = series.label;
        painter.text(
            Pos2::new(x + 11.0, at.y + 6.0),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            palette.text_muted,
        );
        x += 11.0 + 6.5 * text.chars().count() as f32 + 14.0;
    }
}

/// The lowest and highest value of a set of series.
fn value_range(series: &[Series]) -> (f64, f64) {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for series in series {
        for (_, y, _) in &series.points {
            low = low.min(*y);
            high = high.max(*y);
        }
    }
    if !low.is_finite() || !high.is_finite() {
        return (-1.0, 1.0);
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagesync_core::matching::Confidence;

    /// One match point, for the tests below.
    fn point(source_time: f64, reference_time: f64, method: MatchMethod) -> MatchPoint {
        MatchPoint {
            source_time,
            reference_time,
            lag: reference_time - source_time,
            residual_ms: 0.0,
            confidence: Confidence::high(),
            method,
            note: String::new(),
        }
    }

    #[test]
    fn the_drift_of_a_point_is_measured_against_the_model() {
        // The model says the lag grows by 100 ppm of the source time; the point is
        // one millisecond ahead of it at ten seconds, so the local drift is
        // 1 ms / 10 s = 100 ppm on top of the model.
        let model = LinearModel::new(1.000_1, 0.5);
        let measured = point(
            10.0,
            model.reference_time(10.0) + 0.001,
            MatchMethod::FineEnvelope,
        );
        let ppm = drift_ppm_at(&measured, Some(model)).expect("the point is usable");
        assert!((ppm - 100.0).abs() < 1e-6, "the drift is {ppm} ppm");
    }

    #[test]
    fn a_point_on_the_model_has_no_drift() {
        let model = LinearModel::new(1.000_1, 0.5);
        let measured = point(60.0, model.reference_time(60.0), MatchMethod::FineEnvelope);
        let ppm = drift_ppm_at(&measured, Some(model)).expect("the point is usable");
        assert!(ppm.abs() < 1e-9, "the drift is {ppm} ppm");
    }

    #[test]
    fn a_point_at_the_start_of_the_recording_is_not_used() {
        let model = LinearModel::new(1.000_1, 0.5);
        let measured = point(0.5, 1.0, MatchMethod::FineEnvelope);
        assert!(
            drift_ppm_at(&measured, Some(model)).is_none(),
            "a measurement half a second in cannot carry a rate"
        );
    }
}
