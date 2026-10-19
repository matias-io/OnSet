//! OnSet's brand for the egui panels (the launcher and the settings over the show): the same
//! colour tokens as the desktop screens' `brand.css`, Inter for type, and the mark.
//!
//! Violet is for preparation and controls; magenta is kept for what is live or on air.
use std::sync::Arc;

use egui::{Color32, Stroke};

pub const BASE: Color32 = Color32::from_rgb(0x0c, 0x0d, 0x10);
pub const SURFACE: Color32 = Color32::from_rgb(0x13, 0x14, 0x18);
pub const SURFACE_2: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x21);
pub const SURFACE_3: Color32 = Color32::from_rgb(0x22, 0x24, 0x2b);
pub const HOVER: Color32 = Color32::from_rgb(0x27, 0x2a, 0x32);
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x26, 0x2d);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x33, 0x36, 0x3f);
pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xec, 0xf1);
pub const TEXT_2: Color32 = Color32::from_rgb(0xa9, 0xab, 0xb6);
pub const MUTED: Color32 = Color32::from_rgb(0x71, 0x73, 0x7f);
pub const VIOLET: Color32 = Color32::from_rgb(0x8b, 0x7c, 0xf6);
pub const VIOLET_STRONG: Color32 = Color32::from_rgb(0xa1, 0x94, 0xff);
/// Text on violet.
pub const ON_VIOLET: Color32 = Color32::from_rgb(0x0d, 0x0b, 0x1a);
pub const MAGENTA: Color32 = Color32::from_rgb(0xf4, 0x72, 0xb6);
/// Text on magenta.
pub const ON_MAGENTA: Color32 = Color32::from_rgb(0x1f, 0x07, 0x14);
pub const GOOD: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);
pub const WARN: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const BAD: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);

/// The mark's waveform on its 64-unit grid (as in `assets/brand/onset-mark.svg`).
const MARK_LINE: [(f32, f32); 10] = [
    (10.0, 36.0),
    (24.0, 36.0),
    (28.5, 13.0),
    (33.0, 48.0),
    (36.5, 29.0),
    (40.0, 40.0),
    (43.5, 34.0),
    (47.0, 37.5),
    (50.0, 36.0),
    (54.0, 36.0),
];

/// Inter for every panel, and dark visuals built from the brand tokens.
pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
    ctx.set_visuals(visuals());
}

pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = SURFACE;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = BASE;
    v.faint_bg_color = SURFACE_2;
    v.code_bg_color = SURFACE_3;
    v.hyperlink_color = VIOLET_STRONG;
    v.warn_fg_color = WARN;
    v.error_fg_color = BAD;
    v.selection.bg_fill = Color32::from_rgb(0x3b, 0x34, 0x6e);
    v.selection.stroke = Stroke::new(1.0, VIOLET_STRONG);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = SURFACE_2;
    w.noninteractive.weak_bg_fill = SURFACE_2;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.inactive.bg_fill = SURFACE_3;
    w.inactive.weak_bg_fill = SURFACE_3;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_fill = HOVER;
    w.hovered.weak_bg_fill = HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.bg_fill = HOVER;
    w.active.weak_bg_fill = HOVER;
    w.active.bg_stroke = Stroke::new(1.0, VIOLET);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.open.bg_fill = SURFACE_3;
    w.open.weak_bg_fill = SURFACE_3;
    w.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    v
}

/// Paints the mark into `rect` (square). `pulse` (0..1) lifts the spike's dot, so the mark
/// can breathe with the kick.
pub fn paint_mark(painter: &egui::Painter, rect: egui::Rect, pulse: f32) {
    let s = rect.width() / 64.0;
    painter.rect_filled(rect, 15.0 * s, BASE);
    painter.rect_stroke(
        rect,
        15.0 * s,
        Stroke::new(1.0, BORDER_STRONG),
        egui::StrokeKind::Inside,
    );
    let at = |(x, y): (f32, f32)| rect.min + egui::vec2(x * s, y * s);
    let points: Vec<egui::Pos2> = MARK_LINE.iter().copied().map(at).collect();
    let stroke = Stroke::new(3.6 * s, VIOLET_STRONG);
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
        painter.circle_filled(pair[1], 1.8 * s, VIOLET_STRONG);
    }
    painter.circle_filled(points[0], 1.8 * s, VIOLET_STRONG);
    painter.circle_filled(at((28.5, 13.0)), (3.4 + 1.2 * pulse) * s, MAGENTA);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visuals_use_the_brand_surfaces() {
        let v = visuals();
        assert_eq!(v.panel_fill, SURFACE);
        assert_eq!(v.extreme_bg_color, BASE);
        assert_eq!(v.widgets.active.bg_stroke.color, VIOLET);
        assert!(v.dark_mode);
    }
}
