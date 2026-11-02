//! Window chrome colors derived from the active color scheme (like Sublime's adaptive theme).

use egui::{Color32, CornerRadius, Stroke, Visuals};
use syntect::highlighting::Theme;

use crate::syntax_engine::to_color32;

#[derive(Clone, Copy)]
pub struct Chrome {
    pub dark: bool,
    pub editor_bg: Color32,
    pub panel_bg: Color32,
    pub tab_bg: Color32,
    pub hover_bg: Color32,
    pub fg: Color32,
    pub fg_dim: Color32,
    pub accent: Color32,
    pub border: Color32,
}

impl Chrome {
    pub fn from_scheme(theme: &Theme) -> Self {
        let s = &theme.settings;
        let editor_bg = s.background.map(to_color32).unwrap_or(Color32::from_rgb(39, 40, 34));
        let fg = s.foreground.map(to_color32).unwrap_or(Color32::LIGHT_GRAY);
        let dark = luminance(editor_bg) < 0.5;
        let shade = if dark { Color32::BLACK } else { Color32::from_gray(60) };
        Self {
            dark,
            editor_bg,
            panel_bg: mix(editor_bg, shade, if dark { 0.22 } else { 0.08 }),
            tab_bg: mix(editor_bg, shade, if dark { 0.12 } else { 0.05 }),
            hover_bg: mix(editor_bg, fg, 0.10),
            fg,
            fg_dim: mix(fg, editor_bg, 0.40),
            accent: s
                .accent
                .or(s.caret)
                .map(to_color32)
                .unwrap_or(Color32::from_rgb(253, 151, 31)),
            border: mix(editor_bg, shade, if dark { 0.45 } else { 0.18 }),
        }
    }

    pub fn visuals(&self) -> Visuals {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = self.panel_bg;
        v.window_fill = self.panel_bg;
        v.extreme_bg_color = self.editor_bg;
        v.faint_bg_color = self.tab_bg;
        v.window_stroke = Stroke::new(1.0_f32, self.border);
        v.window_corner_radius = CornerRadius::same(6);
        v.menu_corner_radius = CornerRadius::same(4);
        v.selection.bg_fill = self.accent.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0_f32, self.fg);
        v.hyperlink_color = self.accent;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.fg_stroke.color = self.fg;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        v.widgets.noninteractive.bg_fill = self.panel_bg;
        v.widgets.inactive.bg_fill = self.tab_bg;
        v.widgets.inactive.weak_bg_fill = self.tab_bg;
        v.widgets.hovered.bg_fill = self.hover_bg;
        v.widgets.hovered.weak_bg_fill = self.hover_bg;
        v.widgets.active.weak_bg_fill = self.hover_bg;
        v.widgets.open.weak_bg_fill = self.hover_bg;
        v
    }
}

fn luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}
