//! Dependency-free SVG drift graph.
//!
//! The graph is written as plain SVG rather than through a plotting library: it keeps the
//! dependency footprint of the project small, the output is deterministic byte-for-byte,
//! and the same data is re-drawn interactively by the desktop application.

use crate::matching::AlignmentReport;

/// Colours used for the per-source series.
pub const SERIES_COLOURS: [&str; 4] = ["#1f77b4", "#ff7f0e", "#2ca02c", "#d62728"];

/// Stroke styling for a line.
#[derive(Debug, Clone, Copy)]
struct Stroke<'a> {
    /// Stroke colour.
    colour: &'a str,
    /// Stroke width in user units.
    width: f64,
    /// Optional dash pattern.
    dash: Option<&'a str>,
}

impl<'a> Stroke<'a> {
    /// A solid stroke.
    fn solid(colour: &'a str, width: f64) -> Self {
        Self {
            colour,
            width,
            dash: None,
        }
    }
}

/// A very small SVG builder.
struct Svg {
    width: f64,
    height: f64,
    body: String,
}

impl Svg {
    fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            body: String::new(),
        }
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: Option<&str>) {
        self.body.push_str(&format!(
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"{fill}\""
        ));
        if let Some(s) = stroke {
            self.body
                .push_str(&format!(" stroke=\"{s}\" stroke-width=\"1\""));
        }
        self.body.push_str("/>\n");
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: Stroke<'_>) {
        self.body.push_str(&format!(
            "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{}\" stroke-width=\"{:.1}\"",
            stroke.colour, stroke.width
        ));
        if let Some(d) = stroke.dash {
            self.body.push_str(&format!(" stroke-dasharray=\"{d}\""));
        }
        self.body.push_str("/>\n");
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str, opacity: f64) {
        self.body.push_str(&format!(
            "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r:.2}\" fill=\"{fill}\" fill-opacity=\"{opacity:.2}\"/>\n"
        ));
    }

    fn polyline(&mut self, points: &[(f64, f64)], stroke: &str, width: f64, dash: Option<&str>) {
        if points.len() < 2 {
            return;
        }
        let mut d = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 {
                d.push_str(&format!("M {:.2},{:.2}", x, y));
            } else {
                d.push_str(&format!(" L {:.2},{:.2}", x, y));
            }
        }
        self.body.push_str(&format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width:.1}\""
        ));
        if let Some(ds) = dash {
            self.body.push_str(&format!(" stroke-dasharray=\"{ds}\""));
        }
        self.body.push_str("/>\n");
    }

    fn text(&mut self, x: f64, y: f64, content: &str, size: f64, anchor: &str, fill: &str) {
        self.body.push_str(&format!(
            "<text x=\"{x:.2}\" y=\"{y:.2}\" font-family=\"Segoe UI, DejaVu Sans, sans-serif\" \
             font-size=\"{size:.1}\" text-anchor=\"{anchor}\" fill=\"{fill}\">{}</text>\n",
            escape_xml(content)
        ));
    }

    fn finish(self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" \
             width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"{w}\" height=\"{h}\" \
             fill=\"#ffffff\"/>\n{body}</svg>\n",
            w = self.width,
            h = self.height,
            body = self.body
        )
    }
}

/// Escape the characters that matter in XML text nodes.
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A rectangular plotting area with linear axes.
struct Panel {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl Panel {
    fn px(&self, value: f64) -> f64 {
        let t = if (self.x1 - self.x0).abs() < 1e-12 {
            0.0
        } else {
            (value - self.x0) / (self.x1 - self.x0)
        };
        self.x + t.clamp(0.0, 1.0) * self.w
    }

    fn py(&self, value: f64) -> f64 {
        let t = if (self.y1 - self.y0).abs() < 1e-12 {
            0.5
        } else {
            (value - self.y0) / (self.y1 - self.y0)
        };
        self.y + self.h - t.clamp(0.0, 1.0) * self.h
    }
}

/// Write the drift graph as an SVG document.
pub fn render_drift_graph(report: &AlignmentReport) -> String {
    const W: f64 = 1600.0;
    const H: f64 = 900.0;
    let mut svg = Svg::new(W, H);

    // --- Collect the data -----------------------------------------------------
    let mut xs: Vec<f64> = Vec::new();
    let mut lags: Vec<f64> = Vec::new();
    let mut residuals: Vec<f64> = Vec::new();
    for source in &report.sources {
        for p in &source.points {
            if p.confidence.score > 0.0 {
                xs.push(p.source_time);
                lags.push(p.lag);
                residuals.push(p.residual_ms);
            }
        }
    }
    if xs.is_empty() {
        svg.text(
            W / 2.0,
            H / 2.0,
            "no measurements available",
            24.0,
            "middle",
            "#666666",
        );
        return svg.finish();
    }

    let x_min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x0 = x_min - ((x_max - x_min) * 0.02).max(0.5);
    let x1 = x_max + ((x_max - x_min) * 0.02).max(0.5);

    let mut lag_min = lags.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut lag_max = lags.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    for source in &report.sources {
        if let Some(model) = source.calibrated_model.or(source.direct_model) {
            for i in 0..=20 {
                let t = source.source.duration_secs * i as f64 / 20.0;
                let lag = model.reference_time(t) - t;
                lag_min = lag_min.min(lag);
                lag_max = lag_max.max(lag);
            }
        }
    }
    let lag_pad = ((lag_max - lag_min) * 0.1).max(1e-3);
    let lag_lo = lag_min - lag_pad;
    let lag_hi = lag_max + lag_pad;

    let res_min = residuals.iter().cloned().fold(f64::INFINITY, f64::min);
    let res_max = residuals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let res_pad = ((res_max - res_min) * 0.1).max(1.0);
    let res_lo = res_min - res_pad;
    let res_hi = res_max + res_pad;

    // --- Panels ---------------------------------------------------------------
    let upper = Panel {
        x: 110.0,
        y: 70.0,
        w: W - 170.0,
        h: 340.0,
        x0,
        x1,
        y0: lag_lo,
        y1: lag_hi,
    };
    let lower = Panel {
        x: 110.0,
        y: 530.0,
        w: W - 170.0,
        h: 300.0,
        x0,
        x1,
        y0: res_lo,
        y1: res_hi,
    };

    draw_panel(
        &mut svg,
        &upper,
        "Measured offset against source time",
        "source time (s)",
        "offset (s)",
        6,
    );
    draw_panel(
        &mut svg,
        &lower,
        "Residual against the fitted model",
        "source time (s)",
        "residual (ms)",
        5,
    );

    // --- Series ---------------------------------------------------------------
    for (i, source) in report.sources.iter().enumerate() {
        let colour = SERIES_COLOURS[i % SERIES_COLOURS.len()];

        // Measured points (upper panel).
        for p in &source.points {
            if p.confidence.score <= 0.0 {
                continue;
            }
            let cx = upper.px(p.source_time);
            let cy = upper.py(p.lag);
            svg.circle(cx, cy, 2.5, colour, 0.85);
        }

        // Model line (upper panel).
        if let Some(model) = source.calibrated_model.or(source.direct_model) {
            let pts: Vec<(f64, f64)> = (0..=60)
                .map(|k| {
                    let t = source.source.duration_secs * k as f64 / 60.0;
                    (upper.px(t), upper.py(model.reference_time(t) - t))
                })
                .collect();
            svg.polyline(&pts, colour, 2.0, Some("6,4"));
        }

        // Residuals (lower panel).
        for p in &source.points {
            if p.confidence.score <= 0.0 {
                continue;
            }
            let cx = lower.px(p.source_time);
            let cy = lower.py(p.residual_ms);
            svg.circle(cx, cy, 2.5, colour, 0.85);
        }
    }

    // Zero line in the residual panel.
    svg.line(
        lower.x,
        lower.py(0.0),
        lower.x + lower.w,
        lower.py(0.0),
        Stroke::solid("#888888", 1.5),
    );

    // --- Legend ---------------------------------------------------------------
    let mut lx = 120.0;
    for (i, source) in report.sources.iter().enumerate() {
        let colour = SERIES_COLOURS[i % SERIES_COLOURS.len()];
        svg.line(lx, 40.0, lx + 26.0, 40.0, Stroke::solid(colour, 3.0));
        svg.circle(lx + 13.0, 40.0, 3.0, colour, 0.9);
        let label = source.source.file_name.clone();
        svg.text(lx + 32.0, 45.0, &label, 15.0, "start", "#333333");
        lx += 40.0 + label.len() as f64 * 8.0;
    }
    svg.text(
        W / 2.0,
        25.0,
        "StageSync drift graph — reference_time = scale × source_time + offset",
        18.0,
        "middle",
        "#111111",
    );

    svg.finish()
}

/// Draw the frame, grid and tick labels of one panel.
fn draw_panel(
    svg: &mut Svg,
    panel: &Panel,
    title: &str,
    x_label: &str,
    y_label: &str,
    y_ticks: usize,
) {
    svg.text(
        panel.x + panel.w / 2.0,
        panel.y - 22.0,
        title,
        16.0,
        "middle",
        "#222222",
    );

    // Frame and vertical grid.
    svg.rect(panel.x, panel.y, panel.w, panel.h, "none", Some("#333333"));
    for k in 0..=6 {
        let t = k as f64 / 6.0;
        let gx = panel.x + t * panel.w;
        svg.line(
            gx,
            panel.y,
            gx,
            panel.y + panel.h,
            Stroke::solid("#dddddd", 1.0),
        );
        let value = panel.x0 + t * (panel.x1 - panel.x0);
        svg.text(
            gx,
            panel.y + panel.h + 18.0,
            &format_axis(value),
            13.0,
            "middle",
            "#444444",
        );
    }

    // Horizontal grid and labels.
    for k in 0..=y_ticks {
        let t = k as f64 / y_ticks as f64;
        let gy = panel.y + panel.h - t * panel.h;
        svg.line(
            panel.x,
            gy,
            panel.x + panel.w,
            gy,
            Stroke::solid("#eeeeee", 1.0),
        );
        let value = panel.y0 + t * (panel.y1 - panel.y0);
        svg.text(
            panel.x - 8.0,
            gy + 4.0,
            &format_axis(value),
            13.0,
            "end",
            "#444444",
        );
    }

    svg.text(
        panel.x + panel.w / 2.0,
        panel.y + panel.h + 40.0,
        x_label,
        14.0,
        "middle",
        "#333333",
    );
    svg.text(
        panel.x - 62.0,
        panel.y + panel.h / 2.0,
        y_label,
        14.0,
        "middle",
        "#333333",
    );
}

/// Format an axis value compactly.
fn format_axis(value: f64) -> String {
    let a = value.abs();
    if a >= 1000.0 {
        format!("{:.0}", value)
    } else if a >= 10.0 {
        format!("{:.1}", value)
    } else {
        format!("{:.3}", value)
    }
}
