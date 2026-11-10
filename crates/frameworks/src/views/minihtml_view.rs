//! Draws minihtml (`entities::minihtml`, read by the presenter) in egui: popups, phantoms,
//! annotations and completion details. Blocks get their CSS box (padding, margin, border,
//! background, width), tables a grid, `<hr>` a line, `<img>` its picture.

use egui::{Color32, CornerRadius, CursorIcon, FontFamily, FontId, Margin, RichText, Sense, Stroke, TextureHandle, Vec2};
use entities::minihtml::{Block, Content, Line, Sides, Span};
use interface_adapters::view_models::HtmlViewModel;

/// The colors `var(--…)` names in minihtml: the color scheme's.
pub struct HtmlColors {
    pub foreground: Color32,
    pub background: Color32,
    pub link: Color32,
}

/// Draws `html` (wrapping at the width the `ui` has) and returns the `href` of a link clicked.
pub fn show_html(ui: &mut egui::Ui, html: &HtmlViewModel, colors: &HtmlColors, font_size: f32) -> Option<String> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.y = 2.0;
    show_block(ui, &html.block, html, colors, font_size, &mut clicked, &mut 0);
    clicked
}

/// Draws `lines` (wrapping at the width the `ui` has) and returns the `href` of a link clicked.
pub fn show_lines(ui: &mut egui::Ui, lines: &[Line], colors: &HtmlColors, font_size: f32) -> Option<String> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.y = 2.0;
    let html = HtmlViewModel::default();
    for line in lines {
        show_line(ui, line, &html, colors, font_size, &mut clicked);
    }
    clicked
}

fn margin(sides: Sides) -> Margin {
    let side = |v: f32| v.round().clamp(-127.0, 127.0) as i8;
    Margin { left: side(sides.left), right: side(sides.right), top: side(sides.top), bottom: side(sides.bottom) }
}

/// `tables` numbers the tables, for their grids' ids.
fn show_block(ui: &mut egui::Ui, block: &Block, html: &HtmlViewModel, colors: &HtmlColors, font_size: f32, clicked: &mut Option<String>, tables: &mut usize) {
    let style = &block.style;
    let boxed = !style.padding.is_zero() || !style.margin.is_zero() || style.border_width > 0.0 || style.background.is_some() || style.width.is_some();
    let content = |ui: &mut egui::Ui, clicked: &mut Option<String>, tables: &mut usize| {
        if let Some(width) = style.width {
            ui.set_max_width(width);
        }
        for item in &block.content {
            match item {
                Content::Line(line) => show_line(ui, line, html, colors, font_size, clicked),
                Content::Block(inner) => show_block(ui, inner, html, colors, font_size, clicked, tables),
                Content::Rule => {
                    ui.separator();
                }
                Content::Table(rows) => {
                    *tables += 1;
                    egui::Grid::new(("minihtml_table", *tables, rows.len())).spacing(Vec2::new(8.0, 2.0)).show(ui, |ui| {
                        for row in rows {
                            for cell in row {
                                ui.vertical(|ui| show_block(ui, cell, html, colors, font_size, clicked, tables));
                            }
                            ui.end_row();
                        }
                    });
                }
            }
        }
    };
    if !boxed {
        ui.vertical(|ui| content(ui, clicked, tables));
        return;
    }
    let color = |c: &Option<String>| c.as_deref().and_then(|c| parse_color(c, colors));
    let mut frame = egui::Frame::new()
        .inner_margin(margin(style.padding))
        .outer_margin(margin(style.margin))
        .corner_radius(CornerRadius::same(style.border_radius.round().clamp(0.0, 255.0) as u8));
    if let Some(fill) = color(&style.background) {
        frame = frame.fill(fill);
    }
    if style.border_width > 0.0 {
        frame = frame.stroke(Stroke::new(style.border_width, color(&style.border_color).unwrap_or(colors.foreground)));
    }
    frame.show(ui, |ui| content(ui, clicked, tables));
}

fn show_line(ui: &mut egui::Ui, line: &Line, html: &HtmlViewModel, colors: &HtmlColors, font_size: f32, clicked: &mut Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if line.is_empty() {
            ui.label(RichText::new(" ").size(font_size));
        }
        for span in line {
            let response = match &span.image {
                Some(image) => show_image(ui, image, html, font_size).or_else(|| (!span.text.is_empty()).then(|| ui.add(egui::Label::new(styled(span, colors, font_size)).selectable(false)))),
                None => Some(ui.add(egui::Label::new(styled(span, colors, font_size)).selectable(false).sense(if span.link.is_some() { Sense::CLICK } else { Sense::hover() }))),
            };
            if let (Some(response), Some(href)) = (response, &span.link) {
                if response.clicked() {
                    *clicked = Some(href.clone());
                }
                response.on_hover_cursor(CursorIcon::PointingHand);
            }
        }
    });
}

fn styled(span: &Span, colors: &HtmlColors, font_size: f32) -> RichText {
    let size = span.font_size.map_or(font_size, |s| s.resolve(font_size)).clamp(4.0, 96.0);
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
    text
}

/// An `<img>` at its size (the picture's when not given; one dimension keeps the proportions).
/// `None` when its bytes are not there or are not a PNG, JPEG or GIF.
fn show_image(ui: &mut egui::Ui, image: &entities::minihtml::Image, html: &HtmlViewModel, font_size: f32) -> Option<egui::Response> {
    let (_, bytes) = html.images.iter().find(|(src, _)| *src == image.src)?;
    let texture = texture(ui.ctx(), &image.src, bytes)?;
    let natural = texture.size_vec2();
    let size = match (image.width, image.height) {
        (Some(w), Some(h)) => Vec2::new(w, h),
        (Some(w), None) => Vec2::new(w, natural.y * w / natural.x.max(1.0)),
        (None, Some(h)) => Vec2::new(natural.x * h / natural.y.max(1.0), h),
        (None, None) => natural,
    };
    let size = if size.x <= 0.0 || size.y <= 0.0 { Vec2::splat(font_size) } else { size };
    Some(ui.add(egui::Image::new(&texture).fit_to_exact_size(size).sense(Sense::CLICK)))
}

/// The texture of an image, decoded once (kept in egui's memory by `src`). Also the gutter's icons.
pub fn texture(ctx: &egui::Context, src: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let id = egui::Id::new(("minihtml_image", src));
    if let Some(cached) = ctx.data(|d| d.get_temp::<Option<TextureHandle>>(id)) {
        return cached;
    }
    let decoded = image::load_from_memory(bytes).ok().map(|picture| {
        let rgba = picture.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        ctx.load_texture(format!("minihtml:{}", src.chars().take(64).collect::<String>()), color_image, egui::TextureOptions::LINEAR)
    });
    ctx.data_mut(|d| d.insert_temp(id, decoded.clone()));
    decoded
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
        let name = name.split(',').next().unwrap_or_default().trim().trim_start_matches("--");
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
        "silver" => Color32::from_rgb(192, 192, 192),
        "transparent" => Color32::TRANSPARENT,
        _ => return None,
    })
}
