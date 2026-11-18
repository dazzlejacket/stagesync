//! Cohesive studio Light/Dark themes and vector icon painters for StageSync.
//!
//! All colours used across custom painters (waveform timeline, charts, meters,
//! segment spans) come from [`Palette`], ensuring legibility in both Light and
//! Dark modes without hardcoded dark-only assumptions.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};

/// User theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System];

    pub fn from_name(name: &str) -> Self {
        match name {
            "light" => ThemeMode::Light,
            "system" => ThemeMode::System,
            _ => ThemeMode::Dark,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
            ThemeMode::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::System => "System",
        }
    }

    /// Resolve `System` to concrete dark vs light using the context's OS theme.
    pub fn is_dark(self, ctx: &egui::Context) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => ctx
                .input(|i| i.raw.system_theme)
                .map(|t| t == egui::Theme::Dark)
                .unwrap_or_else(|| ctx.theme() == egui::Theme::Dark),
        }
    }
}

/// Complete resolved colour tokens for UI widgets and custom painters.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg_canvas: Color32,
    pub bg_panel: Color32,
    pub bg_surface: Color32,
    pub bg_lane: Color32,
    pub bg_lane_selected: Color32,
    pub bg_strip: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub wave_ref: Color32,
    pub wave_target: Color32,
    pub wave_series_alt: Color32,
    pub playhead: Color32,
    pub loop_fill: Color32,
    pub loop_stroke: Color32,
    pub selection_fill: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub gap_dark: Color32,
    pub beyond: Color32,
    pub ruler_tick: Color32,
    pub grid_line: Color32,
    pub hatch_alpha: Color32,
    pub bar_text: Color32,
}

impl Palette {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg_canvas: Color32::from_rgb(20, 22, 26),
            bg_panel: Color32::from_rgb(27, 30, 36),
            bg_surface: Color32::from_rgb(34, 38, 46),
            bg_lane: Color32::from_rgb(17, 20, 25),
            bg_lane_selected: Color32::from_rgb(25, 34, 48),
            bg_strip: Color32::from_rgb(36, 40, 48),
            border_subtle: Color32::from_rgb(46, 51, 62),
            border_strong: Color32::from_rgb(70, 78, 94),
            text_primary: Color32::from_rgb(230, 232, 238),
            text_muted: Color32::from_rgb(148, 156, 170),
            text_faint: Color32::from_rgb(102, 110, 124),
            accent: Color32::from_rgb(78, 149, 242),
            accent_soft: Color32::from_rgba_unmultiplied(78, 149, 242, 38),
            wave_ref: Color32::from_rgb(108, 172, 252),
            wave_target: Color32::from_rgb(88, 198, 142),
            wave_series_alt: Color32::from_rgb(238, 196, 92),
            playhead: Color32::from_rgb(242, 140, 64),
            loop_fill: Color32::from_rgba_unmultiplied(78, 149, 242, 34),
            loop_stroke: Color32::from_rgb(98, 165, 250),
            selection_fill: Color32::from_rgba_unmultiplied(230, 235, 245, 32),
            ok: Color32::from_rgb(62, 164, 99),
            warn: Color32::from_rgb(214, 168, 44),
            error: Color32::from_rgb(217, 75, 75),
            gap_dark: Color32::from_rgb(122, 31, 31),
            beyond: Color32::from_rgb(108, 114, 126),
            ruler_tick: Color32::from_rgb(92, 100, 114),
            grid_line: Color32::from_rgb(40, 44, 53),
            hatch_alpha: Color32::from_black_alpha(135),
            bar_text: Color32::from_rgb(240, 242, 246),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg_canvas: Color32::from_rgb(236, 238, 242),
            bg_panel: Color32::from_rgb(246, 247, 249),
            bg_surface: Color32::from_rgb(255, 255, 255),
            bg_lane: Color32::from_rgb(250, 251, 253),
            bg_lane_selected: Color32::from_rgb(235, 243, 254),
            bg_strip: Color32::from_rgb(225, 229, 236),
            border_subtle: Color32::from_rgb(206, 212, 222),
            border_strong: Color32::from_rgb(166, 174, 188),
            text_primary: Color32::from_rgb(28, 32, 41),
            text_muted: Color32::from_rgb(84, 92, 108),
            text_faint: Color32::from_rgb(126, 134, 150),
            accent: Color32::from_rgb(37, 99, 235),
            accent_soft: Color32::from_rgba_unmultiplied(37, 99, 235, 28),
            wave_ref: Color32::from_rgb(34, 103, 199),
            wave_target: Color32::from_rgb(28, 138, 86),
            wave_series_alt: Color32::from_rgb(184, 110, 8),
            playhead: Color32::from_rgb(217, 95, 14),
            loop_fill: Color32::from_rgba_unmultiplied(37, 99, 235, 24),
            loop_stroke: Color32::from_rgb(37, 99, 235),
            selection_fill: Color32::from_rgba_unmultiplied(37, 99, 235, 22),
            ok: Color32::from_rgb(34, 139, 78),
            warn: Color32::from_rgb(194, 136, 18),
            error: Color32::from_rgb(201, 54, 54),
            gap_dark: Color32::from_rgb(142, 35, 35),
            beyond: Color32::from_rgb(138, 146, 160),
            ruler_tick: Color32::from_rgb(148, 156, 170),
            grid_line: Color32::from_rgb(224, 228, 236),
            hatch_alpha: Color32::from_rgba_unmultiplied(20, 20, 20, 90),
            bar_text: Color32::from_rgb(252, 253, 255),
        }
    }
}

/// Apply compact, high-craft studio visuals for the chosen theme.
pub fn apply_theme(ctx: &egui::Context, dark: bool) -> Palette {
    let p = Palette::for_mode(dark);
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.window_fill = p.bg_panel;
    visuals.panel_fill = p.bg_panel;
    visuals.faint_bg_color = p.bg_surface;
    visuals.extreme_bg_color = p.bg_lane;
    visuals.code_bg_color = p.bg_surface;
    visuals.warn_fg_color = p.warn;
    visuals.error_fg_color = p.error;
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(p.text_muted);

    visuals.selection.bg_fill = p.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0, p.accent);

    let r = CornerRadius::same(3);
    visuals.window_corner_radius = CornerRadius::same(5);
    visuals.menu_corner_radius = CornerRadius::same(4);

    visuals.widgets.noninteractive.bg_fill = p.bg_surface;
    visuals.widgets.noninteractive.weak_bg_fill = p.bg_panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text_primary);
    visuals.widgets.noninteractive.corner_radius = r;

    visuals.widgets.inactive.bg_fill = p.bg_surface;
    visuals.widgets.inactive.weak_bg_fill = p.bg_surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text_primary);
    visuals.widgets.inactive.corner_radius = r;

    visuals.widgets.hovered.bg_fill = p.accent_soft;
    visuals.widgets.hovered.weak_bg_fill = p.accent_soft;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.2, p.text_primary);
    visuals.widgets.hovered.corner_radius = r;

    // `Visuals::strong_text_color()` reads `widgets.active.fg_stroke.color`,
    // so keep `widgets.active.fg_stroke` set to `p.text_primary` for legibility
    // in both Light and Dark modes.
    visuals.widgets.active.bg_fill = p.accent_soft;
    visuals.widgets.active.weak_bg_fill = p.accent_soft;
    visuals.widgets.active.bg_stroke = Stroke::new(1.2, p.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.text_primary);
    visuals.widgets.active.corner_radius = r;

    visuals.widgets.open.bg_fill = p.bg_surface;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, p.text_primary);
    visuals.widgets.open.corner_radius = r;

    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, visuals);
    ctx.style_mut_of(theme, |style| {
        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.indent = 14.0;
        style.spacing.scroll.bar_width = 8.0;
    });
    p
}

/// Crisp vector icons drawn directly via `Painter` so there are never missing
/// Unicode font glyph boxes on Windows, macOS, or Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Close,
    Sun,
    Moon,
    ZoomIn,
    ZoomOut,
    Fit,
    SkipStart,
    Folder,
    Save,
}

/// Paint a crisp 1px-stroke or filled vector icon inside `rect`.
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let r = (rect.width().min(rect.height()) * 0.36).max(3.0);
    let stroke = Stroke::new(1.5, color);

    match icon {
        Icon::Play => {
            let pts = vec![
                Pos2::new(c.x - r * 0.6, c.y - r * 0.9),
                Pos2::new(c.x + r * 0.95, c.y),
                Pos2::new(c.x - r * 0.6, c.y + r * 0.9),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Stop => {
            let sq = Rect::from_center_size(c, Vec2::splat(r * 1.45));
            painter.rect_filled(sq, 1.0, color);
        }
        Icon::Close => {
            let d = r * 0.72;
            painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], stroke);
            painter.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], stroke);
        }
        Icon::Sun => {
            painter.circle_stroke(c, r * 0.52, stroke);
            for i in 0..8 {
                let a = (i as f32) * std::f32::consts::FRAC_PI_4;
                let (s, cs) = a.sin_cos();
                painter.line_segment(
                    [
                        c + Vec2::new(cs * r * 0.78, s * r * 0.78),
                        c + Vec2::new(cs * r * 1.1, s * r * 1.1),
                    ],
                    Stroke::new(1.2, color),
                );
            }
        }
        Icon::Moon => {
            // Crescent moon polygon oriented cleanly without needing background mask.
            let mut pts = Vec::new();
            for i in 0..=12 {
                let a = -0.35 + (i as f32 / 12.0) * 4.55;
                pts.push(c + Vec2::new(a.cos() * r * 0.82, a.sin() * r * 0.82));
            }
            for i in (0..=10).rev() {
                let a = 0.05 + (i as f32 / 10.0) * 3.75;
                pts.push(
                    c + Vec2::new(r * 0.34, -r * 0.16)
                        + Vec2::new(a.cos() * r * 0.56, a.sin() * r * 0.56),
                );
            }
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.4, color)));
        }
        Icon::ZoomIn => {
            let d = r * 0.75;
            painter.line_segment([c + Vec2::new(-d, 0.0), c + Vec2::new(d, 0.0)], stroke);
            painter.line_segment([c + Vec2::new(0.0, -d), c + Vec2::new(0.0, d)], stroke);
        }
        Icon::ZoomOut => {
            let d = r * 0.75;
            painter.line_segment([c + Vec2::new(-d, 0.0), c + Vec2::new(d, 0.0)], stroke);
        }
        Icon::Fit => {
            let d = r * 0.85;
            painter.line_segment(
                [c + Vec2::new(-d, -d * 0.5), c + Vec2::new(-d, d * 0.5)],
                stroke,
            );
            painter.line_segment(
                [c + Vec2::new(d, -d * 0.5), c + Vec2::new(d, d * 0.5)],
                stroke,
            );
            painter.line_segment(
                [c + Vec2::new(-d * 0.6, 0.0), c + Vec2::new(d * 0.6, 0.0)],
                stroke,
            );
        }
        Icon::SkipStart => {
            let x = c.x - r * 0.65;
            painter.line_segment(
                [Pos2::new(x, c.y - r * 0.85), Pos2::new(x, c.y + r * 0.85)],
                stroke,
            );
            let pts = vec![
                Pos2::new(c.x + r * 0.85, c.y - r * 0.85),
                Pos2::new(c.x - r * 0.3, c.y),
                Pos2::new(c.x + r * 0.85, c.y + r * 0.85),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Folder => {
            let box_rect = Rect::from_center_size(c, Vec2::new(r * 1.8, r * 1.35));
            painter.rect_stroke(box_rect, 1.5, stroke, egui::StrokeKind::Middle);
            painter.line_segment(
                [
                    box_rect.left_top() + Vec2::new(0.0, -2.0),
                    box_rect.left_top() + Vec2::new(r * 0.75, -2.0),
                ],
                stroke,
            );
        }
        Icon::Save => {
            let box_rect = Rect::from_center_size(c, Vec2::splat(r * 1.65));
            painter.rect_stroke(box_rect, 1.5, stroke, egui::StrokeKind::Middle);
            let top = Rect::from_min_max(
                box_rect.left_top() + Vec2::new(2.5, 0.0),
                Pos2::new(box_rect.right() - 2.5, box_rect.top() + r * 0.65),
            );
            painter.rect_stroke(top, 0.0, Stroke::new(1.1, color), egui::StrokeKind::Middle);
        }
    }
}

/// Compact square icon button that still registers an accessible label for automation/tests.
pub fn small_icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    accessible_label: &str,
    tooltip: &str,
) -> egui::Response {
    let size = Vec2::splat(22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), accessible_label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Middle,
        );
        paint_icon(ui.painter(), rect, icon, visuals.text_color());
    }
    response.on_hover_text(tooltip)
}
