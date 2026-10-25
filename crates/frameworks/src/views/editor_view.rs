//! The text area: gutter with line numbers, current line highlight, rulers, find matches and the
//! text itself (an egui `TextEdit` colored by the syntect engine).
//!
//! **The one place a view writes to the model.** egui's `TextEdit` needs `&mut String` and owns
//! typing, caret movement, IME and undo. Copying the whole buffer every frame would be too slow, so
//! the text widget edits `Document::text` in place (via `Editor::text_for_editing`) and reports the
//! caret back with `UiEvent::SelectionChanged`. Selections set by commands flow the other way:
//! when the document's `selection_revision` changes, the view moves the widget's caret.

use std::collections::HashMap;

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align2, Color32, Event, FontId, Id, Key, Modifiers, Rect, Shape, Stroke, TextEdit, Vec2};
use entities::document::DocumentId;
use entities::selection::Selection;
use entities::text::{byte_to_char, char_to_byte, line_col};
use entities::text_commands::CaretMotion;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::EditorViewModel;

use crate::syntax_engine::{to_color32, SharedSyntaxEngine};

/// Sublime font sizes are points; egui sizes are pixels at 96 dpi.
const POINTS_TO_PIXELS: f32 = 4.0 / 3.0;

pub enum EditorText<'a> {
    Editable(&'a mut String),
    ReadOnly(&'a str),
}

/// Which command-made selection each document's widget has already applied.
#[derive(Default)]
pub struct SelectionSync {
    applied_revision: HashMap<DocumentId, u64>,
}

pub fn widget_id(document: DocumentId) -> Id {
    Id::new(("editor", document))
}

pub fn show(
    ui: &mut egui::Ui,
    model: &EditorViewModel,
    text: EditorText,
    engine: &SharedSyntaxEngine,
    sync: &mut SelectionSync,
    events: &mut Vec<UiEvent>,
) {
    let id = widget_id(model.document);
    let ctx = ui.ctx().clone();
    let mut engine = engine.borrow_mut();
    let colors = Colors::from_scheme(&engine.scheme(&model.color_scheme).settings);

    // Like Sublime, the text has the keyboard focus unless a panel, overlay or dialog took it.
    if model.take_focus || ctx.memory(|m| m.focused().is_none()) {
        ctx.memory_mut(|m| m.request_focus(id));
    }

    // Follow selections made by commands (find, go to line, text commands…).
    let mut reveal = false;
    if sync.applied_revision.get(&model.document) != Some(&model.selection_revision) {
        set_widget_selection(&ctx, id, model.selection);
        sync.applied_revision.insert(model.document, model.selection_revision);
        reveal = true;
    }

    // Tab, Shift+Tab and Enter follow Sublime's indentation settings, so the use cases handle them.
    // With several selections (Ctrl+D) the widget only knows the primary one, so typing, deleting
    // and moving the carets go to the use cases too.
    let has_focus = ctx.memory(|m| m.has_focus(id));
    let mut key_events = Vec::new();
    if has_focus && matches!(text, EditorText::Editable(_)) {
        ui.input_mut(|i| {
            if !model.extra_selections.is_empty() {
                key_events.extend(take_multi_selection_keys(i));
            }
            if i.consume_key(Modifiers::NONE, Key::Tab) {
                key_events.push(UiEvent::TabPressed);
            } else if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                key_events.push(UiEvent::ShiftTabPressed);
            } else if model.auto_indent && i.consume_key(Modifiers::NONE, Key::Enter) {
                key_events.push(UiEvent::EnterPressed);
            }
        });
    }

    let font = FontId::monospace(model.font_size * POINTS_TO_PIXELS);
    let row_height = ui.fonts(|f| f.row_height(&font));
    let char_width = ui.fonts(|f| f.glyph_width(&font, '0'));
    {
        let visuals = ui.visuals_mut();
        visuals.selection.bg_fill = colors.selection;
        visuals.selection.stroke = Stroke::new(1.0_f32, colors.foreground);
        visuals.text_cursor.stroke = Stroke::new(2.0_f32, colors.caret);
        visuals.extreme_bg_color = colors.background;
    }

    let (mut read_only_text, editable) = match text {
        EditorText::Editable(text) => ("", Some(text)),
        EditorText::ReadOnly(text) => (text, None),
    };
    let line_count = editable.as_ref().map_or(read_only_text, |t| t.as_str()).matches('\n').count() + 1;
    let gutter_width = if model.line_numbers {
        (line_count.to_string().len().max(3) as f32 + 2.5) * char_width
    } else {
        char_width
    };

    egui::ScrollArea::both()
        .id_salt(("editor_scroll", model.document))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let viewport = ui.clip_rect();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(gutter_width);
                // Placeholder painted under the text once the layout is known.
                let background = ui.painter().add(Shape::Noop);

                let wrap_width = if model.word_wrap {
                    (viewport.width() - gutter_width - char_width * 2.0).max(char_width * 10.0)
                } else {
                    f32::INFINITY
                };
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                    let mut job = engine.layout_job(model.document, text, &model.syntax, &model.color_scheme, font.clone());
                    job.wrap.max_width = wrap_width;
                    ui.fonts(|f| f.layout_job(job))
                };
                let min_height = if model.scroll_past_end {
                    viewport.height() * 2.0 - row_height * 3.0
                } else {
                    viewport.height()
                };
                let buffer: &mut dyn egui::TextBuffer = match editable {
                    Some(text) => text,
                    None => &mut read_only_text,
                };
                let output = TextEdit::multiline(buffer)
                    .id(id)
                    .font(font.clone())
                    .code_editor()
                    .frame(false)
                    .margin(egui::Margin::symmetric(4, 0))
                    .desired_width(if model.word_wrap { wrap_width + 8.0 } else { f32::INFINITY })
                    .min_size(Vec2::new(viewport.width() - gutter_width, min_height))
                    .layouter(&mut layouter)
                    .show(ui);

                let text = output.galley.text();
                let selection = output
                    .cursor_range
                    .map(|r| {
                        let r = r.as_ccursor_range();
                        Selection::new(r.secondary.index, r.primary.index)
                    })
                    .unwrap_or(model.selection);
                if selection != model.selection || output.response.changed() {
                    events.push(UiEvent::SelectionChanged(model.document, selection));
                }

                let galley = &output.galley;
                let origin = output.galley_pos;
                let caret = galley.pos_from_ccursor(CCursor::new(selection.head)).translate(origin.to_vec2());

                let mut shapes = Vec::new();
                if model.highlight_line && selection.is_empty() {
                    shapes.push(Shape::rect_filled(
                        Rect::from_min_max(
                            egui::pos2(viewport.left(), caret.top()),
                            egui::pos2(viewport.right(), caret.top() + row_height),
                        ),
                        0.0,
                        colors.line_highlight,
                    ));
                }
                for &column in &model.rulers {
                    let x = origin.x + column as f32 * char_width;
                    shapes.push(Shape::line_segment(
                        [egui::pos2(x, viewport.top()), egui::pos2(x, viewport.bottom())],
                        Stroke::new(1.0_f32, colors.guide),
                    ));
                }
                shapes.extend(find_match_shapes(model, text, galley, origin, viewport, char_width, colors.find_highlight));
                for extra in &model.extra_selections {
                    for rect in range_rects(galley, extra.start(), extra.end()) {
                        shapes.push(Shape::rect_filled(rect.translate(origin.to_vec2()), 0.0, colors.selection));
                    }
                }
                ui.painter().set(background, Shape::Vec(shapes));
                // The widget draws the primary caret; the extra ones are drawn here, over the text.
                for extra in &model.extra_selections {
                    let caret = galley.pos_from_ccursor(CCursor::new(extra.head)).translate(origin.to_vec2());
                    ui.painter().line_segment([caret.left_top(), caret.left_bottom()], Stroke::new(2.0_f32, colors.caret));
                }

                paint_gutter(ui, model, text, galley, origin, viewport, gutter_width, char_width, row_height, &font, &colors, selection);

                if reveal {
                    ui.scroll_to_rect(caret.expand2(egui::vec2(char_width * 4.0, row_height * 4.0)), None);
                }
            });
        });

    // After SelectionChanged, so the command sees the caret it was pressed at.
    events.extend(key_events);
}

/// Takes the typing, deleting and caret keys out of the input when there are several selections.
fn take_multi_selection_keys(input: &mut egui::InputState) -> Vec<UiEvent> {
    let mut taken = Vec::new();
    input.events.retain(|event| {
        let ui_event = match event {
            Event::Text(text) | Event::Paste(text) => Some(UiEvent::TypedAtSelections(text.clone())),
            Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => match key {
                Key::Backspace => Some(UiEvent::DeleteAtSelections { forward: false }),
                Key::Delete => Some(UiEvent::DeleteAtSelections { forward: true }),
                Key::Enter => Some(UiEvent::TypedAtSelections("\n".into())),
                Key::Tab => Some(UiEvent::TypedAtSelections("\t".into())),
                Key::ArrowLeft => Some(UiEvent::MoveCarets(CaretMotion::Left)),
                Key::ArrowRight => Some(UiEvent::MoveCarets(CaretMotion::Right)),
                Key::Home => Some(UiEvent::MoveCarets(CaretMotion::LineStart)),
                Key::End => Some(UiEvent::MoveCarets(CaretMotion::LineEnd)),
                _ => None,
            },
            _ => None,
        };
        let keep = ui_event.is_none();
        taken.extend(ui_event);
        keep
    });
    taken
}

/// Rectangles covering the chars `start..end`, one per row of the galley.
fn range_rects(galley: &egui::Galley, start: usize, end: usize) -> Vec<Rect> {
    if start == end {
        return Vec::new();
    }
    let a = galley.pos_from_ccursor(CCursor::new(start));
    let b = galley.pos_from_ccursor(CCursor::new(end));
    galley
        .rows
        .iter()
        .filter(|row| row.rect.bottom() > a.top() + 0.5 && row.rect.top() < b.bottom() - 0.5)
        .map(|row| {
            let left = if (row.rect.top() - a.top()).abs() < 0.5 { a.left() } else { row.rect.left() };
            let right = if (row.rect.top() - b.top()).abs() < 0.5 { b.left() } else { row.rect.right() };
            Rect::from_min_max(egui::pos2(left, row.rect.top()), egui::pos2(right, row.rect.bottom()))
        })
        .collect()
}

fn set_widget_selection(ctx: &egui::Context, id: Id, selection: Selection) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(selection.anchor),
        CCursor::new(selection.head),
    )));
    state.store(ctx, id);
}

/// Background boxes for the find matches that are on screen.
fn find_match_shapes(
    model: &EditorViewModel,
    text: &str,
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    char_width: f32,
    color: Color32,
) -> Vec<Shape> {
    if model.find_matches.is_empty() {
        return Vec::new();
    }
    let first_visible = galley.cursor_from_pos(egui::vec2(0.0, viewport.top() - origin.y)).ccursor.index;
    let last_visible = galley.cursor_from_pos(egui::vec2(f32::MAX, viewport.bottom() - origin.y)).ccursor.index;
    let (first_byte, last_byte) = (char_to_byte(text, first_visible), char_to_byte(text, last_visible));

    let start = model.find_matches.partition_point(|m| m.end < first_byte);
    let mut shapes = Vec::new();
    for m in model.find_matches[start..].iter().take_while(|m| m.start <= last_byte) {
        if m.end > text.len() {
            break;
        }
        let begin = byte_to_char(text, m.start);
        let length = text[m.clone()].chars().count();
        let a = galley.pos_from_ccursor(CCursor::new(begin));
        let b = galley.pos_from_ccursor(CCursor::new(begin + length));
        let rect = if (a.top() - b.top()).abs() < 1.0 {
            Rect::from_min_max(a.min, egui::pos2(b.max.x, a.max.y))
        } else {
            Rect::from_min_size(a.min, egui::vec2(char_width, a.height()))
        };
        shapes.push(Shape::rect_filled(rect.translate(origin.to_vec2()).expand(1.0), 2.0, color));
    }
    shapes
}

/// Line numbers, painted over the text so horizontally scrolled text slides under them.
#[allow(clippy::too_many_arguments)]
fn paint_gutter(
    ui: &egui::Ui,
    model: &EditorViewModel,
    text: &str,
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
    font: &FontId,
    colors: &Colors,
    selection: Selection,
) {
    let gutter = Rect::from_min_max(viewport.left_top(), egui::pos2(viewport.left() + gutter_width, viewport.bottom()));
    let painter = ui.painter().with_clip_rect(gutter);
    painter.rect_filled(gutter, 0.0, colors.gutter);
    if !model.line_numbers {
        return;
    }
    let caret_line = line_col(text, selection.head).0;
    let mut line = 0;
    let mut starts_line = true;
    for row in &galley.rows {
        let y = origin.y + row.rect.top();
        if y > viewport.bottom() {
            break;
        }
        if starts_line && y + row_height >= viewport.top() {
            painter.text(
                egui::pos2(gutter.right() - char_width * 1.5, y),
                Align2::RIGHT_TOP,
                (line + 1).to_string(),
                font.clone(),
                if line == caret_line { colors.foreground } else { colors.gutter_foreground },
            );
        }
        starts_line = row.ends_with_newline;
        if starts_line {
            line += 1;
        }
    }
}

/// Editor colors from the color scheme's globals, with sensible fallbacks.
struct Colors {
    background: Color32,
    foreground: Color32,
    caret: Color32,
    selection: Color32,
    line_highlight: Color32,
    gutter: Color32,
    gutter_foreground: Color32,
    guide: Color32,
    find_highlight: Color32,
}

impl Colors {
    fn from_scheme(s: &syntect::highlighting::ThemeSettings) -> Self {
        let background = s.background.map(to_color32).unwrap_or(Color32::from_rgb(39, 40, 34));
        let foreground = s.foreground.map(to_color32).unwrap_or(Color32::LIGHT_GRAY);
        Self {
            background,
            foreground,
            caret: s.caret.map(to_color32).unwrap_or(foreground),
            selection: s.selection.map(to_color32).unwrap_or_else(|| foreground.gamma_multiply(0.25)),
            line_highlight: s.line_highlight.map(to_color32).unwrap_or_else(|| foreground.gamma_multiply(0.06)),
            gutter: s.gutter.map(to_color32).unwrap_or(background),
            gutter_foreground: s.gutter_foreground.map(to_color32).unwrap_or_else(|| foreground.gamma_multiply(0.5)),
            guide: s.guide.map(to_color32).unwrap_or_else(|| foreground.gamma_multiply(0.15)),
            find_highlight: s
                .find_highlight
                .map(to_color32)
                .unwrap_or(Color32::from_rgb(255, 231, 146))
                .gamma_multiply(0.45),
        }
    }
}
