//! Draws minihtml lines (`entities::minihtml`, read by the presenter) in egui: popups and phantoms.

use egui::{Color32, CursorIcon, FontFamily, FontId, RichText, Sense};
use entities::minihtml::Line;

/// The colors `var(--…)` names in minihtml: the color scheme's.
pub struct HtmlColors {
    pub foreground: Color32,
    pub background: Color32,
    pub link: Color32,
}

/// Draws `lines` (wrapping at the width the `ui` has) and returns the `href` of a link clicked.
pub fn show_lines(ui: &mut egui::Ui, lines: &[Line], colors: &HtmlColors, font_size: f32) -> Option<String> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.y = 2.0;
    for line in lines {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if line.is_empty() {
                ui.label(RichText::new(" ").size(font_size));
            }
            for span in line {
                let size = if span.heading { font_size * 1.2 } else { font_size };
                let family = if span.monospace { FontFamily::Monospace } else { FontFamily::Proportional };
                let mut text = RichText::new(&span.text).font(FontId::new(size, family));
                let color = span.color.as_deref().and_then(|c| parse_color(c, colors));
                text = text.color(color.unwrap_or(if span.link.is_some() { colors.link } else { colors.foreground }));
                if let Some(background) = span.background.as_deref().and_then(|c| parse_color(c, colors)) {
                    text = text.background_color(background);
                }
                if span.bold {
                    text = text.strong();
                }
                if span.italic {
                    text = text.italics();
                }
                if span.underline {
                    text = text.underline();
                }
                match &span.link {
                    Some(href) => {
                        let response = ui.add(egui::Label::new(text).selectable(false).sense(Sense::CLICK));
                        if response.clicked() {
                            clicked = Some(href.clone());
                        }
                        response.on_hover_cursor(CursorIcon::PointingHand);
                    }
                    None => {
                        ui.add(egui::Label::new(text).selectable(false));
                    }
                }
            }
        });
    }
    clicked
}

/// A CSS color as minihtml writes it: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` / `rgba()`,
/// a few names, `var(--foreground)` / `var(--background)` / `var(--<color>ish)`, and Sublime's
/// `color(<color> …)` (its adjustments ignored).
pub fn parse_color(text: &str, colors: &HtmlColors) -> Option<Color32> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix("color(").and_then(|t| t.strip_suffix(')')) {
        let first = if inner.starts_with("var(") || inner.starts_with("rgb") {
            inner.find(')').map_or(inner, |end| &inner[..=end])
        } else {
            inner.split_whitespace().next().unwrap_or_default()
        };
        return parse_color(first, colors);
    }
    if let Some(name) = text.strip_prefix("var(").and_then(|t| t.strip_suffix(')')) {
        let name = name.trim().trim_start_matches("--");
        return match name {
            "foreground" => Some(colors.foreground),
            "background" => Some(colors.background),
            "accent" | "bluish" => Some(Color32::from_rgb(100, 150, 230)),
            "redish" => Some(Color32::from_rgb(235, 96, 106)),
            "orangish" => Some(Color32::from_rgb(242, 151, 78)),
            "yellowish" => Some(Color32::from_rgb(230, 200, 90)),
            "greenish" => Some(Color32::from_rgb(130, 200, 120)),
            "cyanish" => Some(Color32::from_rgb(95, 200, 200)),
            "purplish" => Some(Color32::from_rgb(180, 130, 220)),
            "pinkish" => Some(Color32::from_rgb(230, 130, 190)),
            _ => None,
        };
    }
    if let Some(hex) = text.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
        let short = |i: usize| digit(i, 1).map(|v| v * 17);
        return match hex.len() {
            3 => Some(Color32::from_rgb(short(0)?, short(1)?, short(2)?)),
            4 => Some(Color32::from_rgba_unmultiplied(short(0)?, short(1)?, short(2)?, short(3)?)),
            6 => Some(Color32::from_rgb(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            8 => Some(Color32::from_rgba_unmultiplied(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?)),
            _ => None,
        };
    }
    if let Some(inner) = text.strip_prefix("rgba(").or_else(|| text.strip_prefix("rgb(")).and_then(|t| t.strip_suffix(')')) {
        let parts: Vec<f32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        let channel = |i: usize| parts.get(i).map(|v| v.clamp(0.0, 255.0) as u8);
        let alpha = parts.get(3).map_or(255, |a| (a.clamp(0.0, 1.0) * 255.0) as u8);
        return Some(Color32::from_rgba_unmultiplied(channel(0)?, channel(1)?, channel(2)?, alpha));
    }
    Some(match text.to_ascii_lowercase().as_str() {
        "black" => Color32::BLACK,
        "white" => Color32::WHITE,
        "red" => Color32::from_rgb(255, 0, 0),
        "green" => Color32::from_rgb(0, 128, 0),
        "blue" => Color32::from_rgb(0, 0, 255),
        "yellow" => Color32::from_rgb(255, 255, 0),
        "orange" => Color32::from_rgb(255, 165, 0),
        "purple" => Color32::from_rgb(128, 0, 128),
        "pink" => Color32::from_rgb(255, 192, 203),
        "cyan" => Color32::from_rgb(0, 255, 255),
        "gray" | "grey" => Color32::GRAY,
        "transparent" => Color32::TRANSPARENT,
        _ => return None,
    })
}
