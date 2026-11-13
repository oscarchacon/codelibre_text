//! The minimap: the whole text drawn small right of the text area, in the colors of its syntax, with
//! the plugins' regions (except `HIDE_ON_MINIMAP` ones) and the selection. A box marks the part the
//! text area shows (while the mouse is over the minimap, or always with `always_show_minimap_viewport`);
//! clicking or dragging on it scrolls the text there.
//!
//! When the text is taller than the minimap, the minimap scrolls too, in step with the text: at the
//! top of the text it shows its start, at the bottom its end ([`Geometry`]).

use std::ops::Range;

use egui::{Color32, Galley, Rect, Sense, Shape, Stroke};
use interface_adapters::view_models::{MinimapViewModel, RegionViewModel};

/// The minimap's size against the text's.
pub const SCALE: f32 = 0.15;

/// Text areas narrower than this have no minimap.
const MIN_TEXT_WIDTH: f32 = 320.0;

/// How wide the minimap is next to a text area `width` wide (0: none).
pub fn width(width: f32) -> f32 {
    if width < MIN_TEXT_WIDTH {
        0.0
    } else {
        (width * 0.12).clamp(60.0, 140.0)
    }
}

/// The room the vertical scroll bar needs right of the minimap. Floating bars (egui's default) are
/// drawn over the scroll area's content, so the minimap leaves them their lane; solid bars have
/// their own room outside it.
pub fn bar_lane(style: &egui::style::ScrollStyle) -> f32 {
    if style.floating {
        style.bar_width + style.bar_outer_margin
    } else {
        0.0
    }
}

/// Where the minimap is against the text area, in the minimap's own pixels (0 is its top).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// The laid out text's height.
    pub text_height: f32,
    /// What the text area can scroll through (more than the text with `scroll_past_end`).
    pub content_height: f32,
    /// The height of the text area and how far it is scrolled.
    pub viewport_height: f32,
    pub offset: f32,
    /// The minimap's height.
    pub height: f32,
}

impl Geometry {
    fn scroll_max(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// How much of the small text does not fit in the minimap.
    fn overflow(&self) -> f32 {
        (self.text_height * SCALE - self.height).max(0.0)
    }

    /// How far the box moves in the minimap for each pixel the text scrolls.
    fn rate(&self) -> f32 {
        let scroll_max = self.scroll_max();
        if scroll_max <= 0.0 || self.overflow() <= 0.0 {
            SCALE
        } else {
            (SCALE - self.overflow() / scroll_max).max(0.001)
        }
    }

    /// How far the minimap itself is scrolled: in step with the text.
    pub fn scroll(&self) -> f32 {
        let scroll_max = self.scroll_max();
        if scroll_max <= 0.0 {
            0.0
        } else {
            self.overflow() * (self.offset / scroll_max).clamp(0.0, 1.0)
        }
    }

    /// The box of the part the text area shows, from its top to its bottom.
    pub fn viewport_box(&self) -> Range<f32> {
        let top = self.offset * SCALE - self.scroll();
        top..top + self.viewport_height * SCALE
    }

    /// The text area's scroll that puts the middle of the box at `y`.
    pub fn offset_for_pointer(&self, y: f32) -> f32 {
        let half_box = self.viewport_height * SCALE / 2.0;
        ((y - half_box) / self.rate()).clamp(0.0, self.scroll_max())
    }
}

/// The colors the minimap draws with besides the text's.
pub struct Colors {
    pub background: Color32,
    pub foreground: Color32,
    pub selection: Color32,
    /// The color scheme's `minimap_border`, if it has one.
    pub border: Option<Color32>,
}

/// What the text area laid out and drew this frame.
pub struct Text<'a> {
    pub galley: &'a Galley,
    pub row_height: f32,
    pub selection: Range<usize>,
    pub regions: &'a [RegionViewModel],
    pub region_colors: &'a [Color32],
}

/// Draws the minimap in `area`; returns the text area's new scroll when it was clicked or dragged.
pub fn show(ui: &egui::Ui, id: egui::Id, area: Rect, settings: &MinimapViewModel, text: &Text, geometry: Geometry, colors: &Colors) -> Option<f32> {
    let response = ui.interact(area, id, Sense::CLICK | Sense::DRAG);
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(area, 0.0, colors.background);
    let scroll = geometry.scroll();
    let rows = RowStarts::new(text.galley);
    let shown = scroll / SCALE..(scroll + area.height()) / SCALE;
    let to_screen = |x: f32, y: f32| egui::pos2(area.left() + x * SCALE, area.top() + y * SCALE - scroll);
    let line = (text.row_height * SCALE).max(1.0);

    let mut shapes = Vec::new();
    // The marks under the text: the selection and the regions.
    let mut mark = |range: &Range<usize>, color: Color32| {
        for rect in rows.rects(text.galley, range, shown.clone()) {
            let min = to_screen(rect.left(), rect.top());
            let max = to_screen(rect.right(), rect.bottom());
            let max = egui::pos2(max.x.max(min.x + 2.0), max.y.max(min.y + line));
            shapes.push(Shape::rect_filled(Rect::from_min_max(min, max), 0.0, color));
        }
    };
    if !text.selection.is_empty() {
        mark(&text.selection, colors.selection);
    }
    for (regions, &color) in text.regions.iter().zip(text.region_colors).filter(|(r, _)| r.on_minimap) {
        for range in &regions.ranges {
            mark(range, color.gamma_multiply(0.6));
        }
    }
    // The text, a bar for each word in its color.
    let sections = &text.galley.job.sections;
    for row in text.galley.rows.iter().filter(|r| r.rect.bottom() >= shown.start && r.rect.top() <= shown.end) {
        let mut word: Option<(f32, f32, u32)> = None;
        let mut flush = |word: &mut Option<(f32, f32, u32)>| {
            if let Some((start, end, section)) = word.take() {
                let color = sections.get(section as usize).map_or(colors.foreground, |s| s.format.color);
                let min = to_screen(start, row.rect.top());
                shapes.push(Shape::rect_filled(
                    Rect::from_min_max(min, egui::pos2(area.left() + end * SCALE, min.y + line * 0.7)),
                    0.0,
                    color.gamma_multiply(0.75),
                ));
            }
        };
        for glyph in &row.glyphs {
            if glyph.chr.is_whitespace() {
                flush(&mut word);
                continue;
            }
            let right = glyph.pos.x + glyph.advance_width;
            match &mut word {
                Some((_, end, section)) if *section == glyph.section_index => *end = right,
                _ => {
                    flush(&mut word);
                    word = Some((glyph.pos.x, right, glyph.section_index));
                }
            }
        }
        flush(&mut word);
    }
    painter.extend(shapes);

    // The part the text area shows.
    let hovered = response.hovered() || response.dragged();
    if settings.always_show_viewport || hovered {
        let span = geometry.viewport_box();
        let rect = Rect::from_min_max(egui::pos2(area.left(), area.top() + span.start), egui::pos2(area.right(), area.top() + span.end));
        painter.rect_filled(rect, 0.0, colors.foreground.gamma_multiply(if hovered { 0.12 } else { 0.07 }));
        if settings.draw_border {
            let border = colors.border.unwrap_or_else(|| colors.foreground.gamma_multiply(0.4));
            painter.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
        }
    }

    let pointer = response.interact_pointer_pos().filter(|_| response.is_pointer_button_down_on() || response.clicked())?;
    Some(geometry.offset_for_pointer(pointer.y - area.top()))
}

/// The first char of each row of a galley, to find a char's row without walking the rows.
struct RowStarts(Vec<usize>);

impl RowStarts {
    fn new(galley: &Galley) -> Self {
        let mut start = 0;
        Self(
            galley
                .rows
                .iter()
                .map(|row| {
                    let this = start;
                    start += row.char_count_including_newline();
                    this
                })
                .collect(),
        )
    }

    fn row_of(&self, char: usize) -> usize {
        self.0.partition_point(|&start| start <= char).saturating_sub(1)
    }

    /// The x of a char in its row.
    fn x(&self, galley: &Galley, row: usize, char: usize) -> f32 {
        let row_data = &galley.rows[row];
        let column = char - self.0[row];
        row_data.glyphs.get(column).map_or(row_data.rect.right(), |g| g.pos.x)
    }

    /// The rectangles (in galley coordinates) of a char range, one per row, only of rows that
    /// reach into `shown` (a span of y).
    fn rects(&self, galley: &Galley, range: &Range<usize>, shown: Range<f32>) -> Vec<Rect> {
        if galley.rows.is_empty() {
            return Vec::new();
        }
        let (first, last) = (self.row_of(range.start), self.row_of(range.end));
        (first..=last)
            .filter(|&row| galley.rows[row].rect.bottom() >= shown.start && galley.rows[row].rect.top() <= shown.end)
            .map(|row| {
                let rect = galley.rows[row].rect;
                let left = if row == first { self.x(galley, row, range.start) } else { 0.0 };
                let right = if row == last { self.x(galley, row, range.end) } else { rect.right() };
                Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right.max(left), rect.bottom()))
            })
            .collect()
    }
}
