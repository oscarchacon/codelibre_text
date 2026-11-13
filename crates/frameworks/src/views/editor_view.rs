//! The text area: gutter with line numbers, current line highlight, rulers, find matches, plugins'
//! regions and icons, and the text itself (an egui `TextEdit` colored by the syntect engine).
//!
//! **The one place a view writes to the model.** egui's `TextEdit` needs `&mut String` and owns
//! typing, caret movement, IME and undo. Copying the whole buffer every frame would be too slow, so
//! the text widget edits `Document::text` in place (via `Editor::text_for_editing`) and reports the
//! caret back with `UiEvent::SelectionChanged`. Selections set by commands flow the other way:
//! when the document's `selection_revision` changes, the view moves the widget's caret.

use std::collections::HashMap;
use std::ops::Range;

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align2, Color32, Event, FontId, Id, Key, Modifiers, Rect, Shape, Stroke, TextEdit, Vec2};
use entities::document::DocumentId;
use entities::menu::ContextMenuTarget;
use entities::selection::Selection;
use entities::text::{byte_to_char, char_to_byte, line_col};
use entities::text_commands::CaretMotion;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{
    AutoCompleteViewModel, ContextMenuViewModel, CompletionViewModel, EditorViewModel, PhantomLayout, PhantomViewModel, PopupViewModel, RegionViewModel, Underline,
};
use use_cases::plugin_popups::HoverZone;

use crate::syntax_engine::{to_color32, SharedSyntaxEngine};
use crate::views::{context_menu, minimap};
use crate::views::minihtml_view::{self, show_html, HtmlColors};

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
    /// The same for plugins' `view.show` requests.
    applied_show: HashMap<DocumentId, u64>,
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
    menu: Option<&ContextMenuViewModel>,
    events: &mut Vec<UiEvent>,
) {
    let id = widget_id(model.document);
    let ctx = ui.ctx().clone();
    let mut engine = engine.borrow_mut();
    let colors = Colors::from_scheme(&engine.scheme(&model.color_scheme).settings);
    let region_colors: Vec<Color32> = model
        .regions
        .iter()
        .map(|r| engine.scope_color(&model.color_scheme, &r.scope).unwrap_or(colors.foreground))
        .collect();

    // Like Sublime, the text of the focused group has the keyboard focus unless a panel, overlay
    // or dialog took it.
    if model.active && (model.take_focus || ctx.memory(|m| m.focused().is_none())) {
        ctx.memory_mut(|m| m.request_focus(id));
    }

    // Follow selections made by commands (find, go to line, text commands…).
    let mut reveal = false;
    if sync.applied_revision.get(&model.document) != Some(&model.selection_revision) {
        set_widget_selection(&ctx, id, model.selection);
        sync.applied_revision.insert(model.document, model.selection_revision);
        reveal = true;
    }
    // And `view.show` from plugins.
    let show = model.show.filter(|&(_, _, revision)| sync.applied_show.get(&model.document) != Some(&revision));
    if let Some((_, _, revision)) = show {
        sync.applied_show.insert(model.document, revision);
    }

    // Tab, Shift+Tab and Enter follow Sublime's indentation settings, so the use cases handle them.
    // With several selections (Ctrl+D) the widget only knows the primary one, so typing, deleting
    // and moving the carets go to the use cases too.
    let has_focus = ctx.memory(|m| m.has_focus(id));
    let mut key_events = Vec::new();
    // What the widget is about to type (it may open the auto-complete list).
    let mut typed = String::new();
    if has_focus && matches!(text, EditorText::Editable(_)) {
        ui.input_mut(|i| {
            if !model.extra_selections.is_empty() {
                key_events.extend(take_multi_selection_keys(i));
            }
            for event in &i.events {
                if let Event::Text(text) = event {
                    typed.push_str(text);
                }
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
    // A column for the regions' icons, left of the line numbers, while some region set has one.
    let gutter_width = gutter_width + if has_icons(model) { icon_size(row_height) } else { 0.0 };

    // The minimap takes a strip at the right; the text and its scroll bars the rest.
    let full = ui.available_rect_before_wrap();
    let minimap_width = if model.minimap.is_some() { minimap::width(full.width()) } else { 0.0 };
    let text_rect = Rect::from_min_max(full.min, egui::pos2(full.right() - minimap_width, full.bottom()));
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(*ui.layout()));
    let scroll = egui::ScrollArea::both()
        .id_salt(("editor_scroll", model.document))
        .auto_shrink([false, false])
        .show(&mut text_ui, |ui| {
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
                // Phantoms take room in the text: their sizes from the last frame.
                let phantom_sizes: Vec<Vec2> = model
                    .phantoms
                    .iter()
                    .map(|p| ctx.memory(|m| m.area_rect(phantom_id(model.document, p.id))).map_or(Vec2::ZERO, |r| r.size()))
                    .collect();
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                    let mut job = engine.layout_job(model.document, text, &model.syntax, &model.color_scheme, font.clone());
                    job.wrap.max_width = wrap_width;
                    make_room_for_phantoms(&mut job, text, model, &phantom_sizes, row_height, char_width);
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

                // A click in another group's text gives that group the focus.
                if !model.active && (output.response.clicked() || output.response.gained_focus()) {
                    events.push(UiEvent::FocusGroup(model.group));
                }
                let text = output.galley.text();
                let selection = output
                    .cursor_range
                    .map(|r| {
                        let r = r.as_ccursor_range();
                        Selection::new(r.secondary.index, r.primary.index)
                    })
                    .unwrap_or(model.selection);
                if output.response.changed() {
                    events.push(UiEvent::TextEdited(model.document));
                }
                if selection != model.selection || output.response.changed() {
                    events.push(UiEvent::SelectionChanged(model.document, selection));
                }
                if output.response.changed() && !typed.is_empty() {
                    events.push(UiEvent::TextTyped(model.document, std::mem::take(&mut typed)));
                }

                let galley = &output.galley;
                let origin = output.galley_pos;
                // A right click: the menu of the char clicked (its view takes the focus first).
                let menu_point = Id::new(("context_menu_point", model.document));
                if output.response.secondary_clicked() {
                    if !model.active {
                        events.push(UiEvent::FocusGroup(model.group));
                    }
                    if let Some(pointer) = output.response.interact_pointer_pos() {
                        let point = galley.cursor_from_pos(pointer - origin).ccursor.index;
                        ctx.data_mut(|d| d.insert_temp(menu_point, point));
                    }
                }
                let point = ctx.data(|d| d.get_temp::<usize>(menu_point)).unwrap_or(selection.head);
                let target = ContextMenuTarget::Text { document: model.document, point };
                context_menu::attach(&output.response, target, menu, events);
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
                let visible = visible_chars(galley, origin, viewport);
                events.push(UiEvent::VisibleRange(model.document, visible.start, visible.end));
                for (regions, &color) in model.regions.iter().zip(&region_colors) {
                    shapes.extend(region_shapes(regions, color, galley, origin, visible.clone(), char_width));
                }
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
                paint_region_icons(ui, model, &region_colors, galley, origin, viewport, gutter_width, char_width, row_height);

                let html_colors = HtmlColors {
                    foreground: colors.foreground,
                    background: colors.background,
                    link: colors.caret,
                };
                let html_size = font.size * 0.9;
                show_phantoms(ui, model, text, galley, origin, viewport, &phantom_sizes, (char_width, row_height), html_size, &html_colors, &colors, events);
                show_annotations(ui, model, &region_colors, galley, origin, viewport, char_width, html_size, &html_colors, &colors, events);
                if let Some(popup) = &model.popup {
                    show_popup(ui, model.document, popup, galley, origin, viewport, html_size, &html_colors, &colors, events);
                }
                if let Some(list) = &model.auto_complete {
                    show_auto_complete(ui, model.document, list, galley, origin, viewport, &font, row_height, &html_colors, &colors, events);
                }
                if model.report_hover {
                    report_hover(ui, model, &model.phantoms, galley, origin, viewport, gutter_width, events);
                }

                if reveal {
                    ui.scroll_to_rect(caret.expand2(egui::vec2(char_width * 4.0, row_height * 4.0)), None);
                }
                if let Some((point, center, _)) = show {
                    let at = galley.pos_from_ccursor(CCursor::new(point)).translate(origin.to_vec2());
                    let align = center.then_some(egui::Align::Center);
                    ui.scroll_to_rect(at.expand2(egui::vec2(char_width * 4.0, row_height * 2.0)), align);
                }
                (galley.clone(), selection)
            })
            .inner
        });
    ui.allocate_rect(full, egui::Sense::hover());

    if let Some(settings) = model.minimap.as_ref().filter(|_| minimap_width > 0.0) {
        let area = Rect::from_min_max(egui::pos2(text_rect.right(), full.top()), full.max);
        let (galley, selection) = &scroll.inner;
        let geometry = minimap::Geometry {
            text_height: galley.rect.height(),
            content_height: scroll.content_size.y,
            viewport_height: scroll.inner_rect.height(),
            offset: scroll.state.offset.y,
            height: area.height(),
        };
        let text = minimap::Text {
            galley,
            row_height,
            selection: selection.start()..selection.end(),
            regions: &model.regions,
            region_colors: &region_colors,
        };
        let minimap_colors = minimap::Colors {
            background: colors.background,
            foreground: colors.foreground,
            selection: colors.selection,
            border: colors.minimap_border,
        };
        if let Some(offset) = minimap::show(ui, id.with("minimap"), area, settings, &text, geometry, &minimap_colors) {
            let mut state = scroll.state;
            state.offset.y = offset;
            state.store(&ctx, scroll.id);
            ctx.request_repaint();
        }
    }

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
    let visible = visible_chars(galley, origin, viewport);
    let (first_byte, last_byte) = (char_to_byte(text, visible.start), char_to_byte(text, visible.end));

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

/// The chars on screen: from the first visible row's start to the last one's end.
fn visible_chars(galley: &egui::Galley, origin: egui::Pos2, viewport: Rect) -> Range<usize> {
    let first = galley.cursor_from_pos(egui::vec2(0.0, viewport.top() - origin.y)).ccursor.index;
    let last = galley.cursor_from_pos(egui::vec2(f32::MAX, viewport.bottom() - origin.y)).ccursor.index;
    first..last
}

/// A plugin's regions that are on screen: a fill, an outline, an underline, or a bar for empty ones.
fn region_shapes(
    regions: &RegionViewModel,
    color: Color32,
    galley: &egui::Galley,
    origin: egui::Pos2,
    visible: Range<usize>,
    char_width: f32,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    let stroke = Stroke::new(1.0_f32, color);
    for range in regions.ranges.iter().filter(|r| r.end >= visible.start && r.start <= visible.end) {
        if range.is_empty() {
            if regions.overwrite_empty {
                // `DRAW_EMPTY_AS_OVERWRITE`: a block where the next char would go.
                let at = galley.pos_from_ccursor(CCursor::new(range.start)).translate(origin.to_vec2());
                let block = Rect::from_min_max(at.left_top(), at.left_bottom() + egui::vec2(char_width, 0.0));
                shapes.push(Shape::rect_filled(block, 0.0, color.gamma_multiply(0.5)));
            } else if regions.draw_empty {
                let at = galley.pos_from_ccursor(CCursor::new(range.start)).translate(origin.to_vec2());
                shapes.push(Shape::line_segment([at.left_top(), at.left_bottom()], stroke));
            }
            continue;
        }
        for rect in range_rects(galley, range.start, range.end) {
            let rect = rect.translate(origin.to_vec2());
            if regions.fill {
                shapes.push(Shape::rect_filled(rect, 2.0, color.gamma_multiply(0.25)));
            }
            if regions.outline {
                shapes.push(Shape::rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside));
            }
            if let Some(underline) = regions.underline {
                shapes.extend(underline_shapes(underline, rect, stroke));
            }
        }
    }
    shapes
}

fn underline_shapes(underline: Underline, rect: Rect, stroke: Stroke) -> Vec<Shape> {
    let y = rect.bottom() - 1.0;
    match underline {
        Underline::Solid => vec![Shape::line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], stroke)],
        Underline::Stippled => {
            let mut dashes = Vec::new();
            let mut x = rect.left();
            while x < rect.right() {
                dashes.push(Shape::line_segment([egui::pos2(x, y), egui::pos2((x + 2.0).min(rect.right()), y)], stroke));
                x += 4.0;
            }
            dashes
        }
        Underline::Squiggly => {
            let mut points = Vec::new();
            let mut x = rect.left();
            let mut up = true;
            while x <= rect.right() {
                points.push(egui::pos2(x, if up { y - 2.0 } else { y }));
                x += 2.0;
                up = !up;
            }
            vec![Shape::line(points, stroke)]
        }
    }
}

fn has_icons(model: &EditorViewModel) -> bool {
    model.regions.iter().any(|r| !r.icon.is_empty() && !r.ranges.is_empty())
}

/// The side of a gutter icon: a bit less than a line, as Sublime's 16 px icons in 19 px lines.
fn icon_size(row_height: f32) -> f32 {
    (row_height * 0.8).round().max(8.0)
}

/// Gutter icons of plugins' regions (`dot`, `circle`, `bookmark`, pictures of resources; other icons are drawn as dots),
/// on the line where each region starts.
#[allow(clippy::too_many_arguments)]
fn paint_region_icons(
    ui: &egui::Ui,
    model: &EditorViewModel,
    region_colors: &[Color32],
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
) {
    let gutter = Rect::from_min_max(viewport.left_top(), egui::pos2(viewport.left() + gutter_width, viewport.bottom()));
    let painter = ui.painter().with_clip_rect(gutter);
    let visible = visible_chars(galley, origin, viewport);
    let size = icon_size(row_height);
    let x = gutter.left() + char_width * 0.3 + size / 2.0;
    let radius = (row_height * 0.22).max(2.0);
    for (regions, &color) in model.regions.iter().zip(region_colors) {
        if regions.icon.is_empty() {
            continue;
        }
        // A picture is tinted with the region's color, as Sublime does (white icons take it).
        let texture = regions.icon_image.as_ref().and_then(|bytes| minihtml_view::texture(ui.ctx(), &regions.icon, bytes));
        for range in regions.ranges.iter().filter(|r| r.end >= visible.start && r.start <= visible.end) {
            let y = galley.pos_from_ccursor(CCursor::new(range.start)).translate(origin.to_vec2()).center().y;
            let center = egui::pos2(x, y);
            if let Some(texture) = &texture {
                let rect = Rect::from_center_size(center, Vec2::splat(size));
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(texture.id(), rect, uv, color.to_opaque());
                continue;
            }
            match regions.icon.as_str() {
                "circle" => {
                    painter.circle_stroke(center, radius, Stroke::new(1.5_f32, color));
                }
                "bookmark" => {
                    let rect = Rect::from_center_size(center, egui::vec2(radius * 1.4, radius * 2.2));
                    painter.rect_filled(rect, 1.0, color);
                }
                _ => {
                    painter.circle_filled(center, radius, color);
                }
            }
        }
    }
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

/// How long the mouse rests before `on_hover`, in seconds (Sublime's is about this).
const HOVER_DELAY: f32 = 0.5;

fn phantom_id(document: DocumentId, phantom: u64) -> Id {
    Id::new(("plugin_phantom", document, phantom))
}

/// The room phantoms take, as Sublime gives it: an inline one (`LAYOUT_INLINE`) pushes the text
/// after its point aside (`leading_space` before that char); those below a line (`LAYOUT_BELOW`,
/// `LAYOUT_BLOCK`) make the line's last row taller (a char of it gets a taller `line_height`, and
/// the text keeps to the top of rows), the phantoms drawn in the room left under the text.
fn make_room_for_phantoms(job: &mut egui::text::LayoutJob, text: &str, model: &EditorViewModel, sizes: &[Vec2], row_height: f32, char_width: f32) {
    let mut below: Vec<(usize, f32)> = Vec::new();
    let mut inline: Vec<(usize, f32)> = Vec::new();
    for (phantom, size) in model.phantoms.iter().zip(sizes) {
        let byte = char_to_byte(text, phantom.point);
        if *size == Vec2::ZERO {
            continue;
        }
        match phantom.layout {
            PhantomLayout::Inline if byte < text.len() && !text[byte..].starts_with('\n') => inline.push((byte, size.x + char_width)),
            PhantomLayout::Inline => {}
            PhantomLayout::Below | PhantomLayout::Block => {
                let line_end = text[byte..].find('\n').map_or(text.len(), |at| byte + at);
                match below.iter_mut().find(|(end, _)| *end == line_end) {
                    Some((_, height)) => *height += size.y + 2.0,
                    None => below.push((line_end, size.y + 2.0)),
                }
            }
        }
    }
    if inline.is_empty() && below.is_empty() {
        return;
    }
    for (byte, width) in inline {
        if let Some(section) = isolate(job, byte, false) {
            job.sections[section].leading_space += width;
        }
    }
    if !below.is_empty() {
        for section in &mut job.sections {
            section.format.valign = egui::Align::TOP;
        }
    }
    for (line_end, height) in below {
        let line_start = text[..line_end].rfind('\n').map_or(0, |at| at + 1);
        // The line's last char, or for an empty line the `\n` before it (the height of an empty
        // row is that of the line break that starts it).
        let target = if line_end > line_start {
            text[..line_end].char_indices().next_back().map(|(at, _)| at)
        } else {
            line_start.checked_sub(1).or((!text.is_empty()).then_some(0))
        };
        if let Some(section) = target.and_then(|at| isolate(job, at, true)) {
            job.sections[section].format.line_height = Some(row_height + height);
        }
    }
}

/// Splits the job's sections so that one starts at `byte` (and, with `one_char`, ends after that
/// char); its index.
fn isolate(job: &mut egui::text::LayoutJob, byte: usize, one_char: bool) -> Option<usize> {
    let split = |job: &mut egui::text::LayoutJob, at: usize| -> Option<usize> {
        let index = job.sections.iter().position(|s| s.byte_range.start <= at && at < s.byte_range.end)?;
        if job.sections[index].byte_range.start == at {
            return Some(index);
        }
        let mut second = job.sections[index].clone();
        second.byte_range.start = at;
        second.leading_space = 0.0;
        job.sections[index].byte_range.end = at;
        job.sections.insert(index + 1, second);
        Some(index + 1)
    };
    let index = split(job, byte)?;
    if one_char {
        let next = job.text[byte..].chars().next().map_or(byte, |c| byte + c.len_utf8());
        if next < job.text.len() {
            split(job, next);
        }
    }
    Some(index)
}

/// Plugins' phantoms on screen, in the room `make_room_for_phantoms` made: inline ones before
/// the char of their point (or after the line's end), the others under their line, one below
/// the other (`LAYOUT_BELOW` from their point's column, `LAYOUT_BLOCK` from the left edge). Their
/// sizes this frame make the room of the next one.
#[allow(clippy::too_many_arguments)]
fn show_phantoms(
    ui: &egui::Ui,
    model: &EditorViewModel,
    text: &str,
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    sizes: &[Vec2],
    (char_width, row_height): (f32, f32),
    font_size: f32,
    html_colors: &HtmlColors,
    colors: &Colors,
    events: &mut Vec<UiEvent>,
) {
    let visible = visible_chars(galley, origin, viewport);
    // How far down the room under each line is used, by the line's end.
    let mut used_below: HashMap<usize, f32> = HashMap::new();
    let mut next_x_inline: HashMap<usize, f32> = HashMap::new();
    for (phantom, size) in model.phantoms.iter().zip(sizes) {
        if phantom.point < visible.start.saturating_sub(1) || phantom.point > visible.end {
            continue;
        }
        let at = galley.pos_from_ccursor(CCursor::new(phantom.point)).translate(origin.to_vec2());
        let byte = char_to_byte(text, phantom.point);
        let position = match phantom.layout {
            PhantomLayout::Inline if byte < text.len() && !text[byte..].starts_with('\n') => {
                let x = next_x_inline.entry(phantom.point).or_insert(at.left() - size.x - char_width * 0.5);
                let position = egui::pos2(*x, at.top());
                *x += size.x + char_width;
                position
            }
            PhantomLayout::Inline => egui::pos2(at.left() + char_width, at.top()),
            PhantomLayout::Below | PhantomLayout::Block => {
                let line_end = text[byte..].find('\n').map_or(text.len(), |offset| byte + offset);
                let end = galley.pos_from_ccursor(CCursor::new(byte_to_char(text, line_end))).translate(origin.to_vec2());
                let used = used_below.entry(line_end).or_insert(0.0);
                let x = if phantom.layout == PhantomLayout::Below { at.left() } else { origin.x };
                let position = egui::pos2(x, end.top() + row_height + *used);
                *used += size.y + 2.0;
                position
            }
        };
        let response = egui::Area::new(phantom_id(model.document, phantom.id))
            .order(egui::Order::Middle)
            .fixed_pos(position)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(viewport);
                egui::Frame::new()
                    .fill(blend(colors.background, colors.foreground, 0.08))
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(4, 0))
                    .show(ui, |ui| {
                        ui.set_max_width((viewport.right() - position.x - 8.0).max(char_width * 20.0));
                        show_html(ui, &phantom.html, html_colors, font_size)
                    })
                    .inner
            });
        if let Some(href) = response.inner {
            events.push(UiEvent::PhantomLinkClicked { phantom: phantom.id, href });
        }
        // Its room in the text follows its size from the next frame on.
        if (response.response.rect.size() - *size).length() > 0.5 {
            ui.ctx().request_repaint();
        }
    }
}

/// The annotations of plugins' regions, at the right edge of the view on the row of their region
/// (several on one row stacked leftwards), with a bar of the region's color (or the plugin's
/// `annotation_color`) on their left, as Sublime draws them. Over the text when the line is long.
#[allow(clippy::too_many_arguments)]
fn show_annotations(
    ui: &egui::Ui,
    model: &EditorViewModel,
    region_colors: &[Color32],
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    char_width: f32,
    font_size: f32,
    html_colors: &HtmlColors,
    colors: &Colors,
    events: &mut Vec<UiEvent>,
) {
    let visible = visible_chars(galley, origin, viewport);
    let mut next_right: HashMap<usize, f32> = HashMap::new();
    for (set, &scope_color) in model.regions.iter().zip(region_colors) {
        let color = minihtml_view::parse_color(&set.annotation_color, html_colors).unwrap_or(scope_color);
        for (index, annotation) in set.annotations.iter().enumerate().filter(|(_, a)| visible.contains(&a.point) || a.point == visible.end) {
            let at = galley.pos_from_ccursor(CCursor::new(annotation.point));
            let Some((row_index, row)) = galley.rows.iter().enumerate().find(|(_, r)| (r.rect.top() - at.top()).abs() < 0.5) else {
                continue;
            };
            let right = *next_right.entry(row_index).or_insert(viewport.right() - char_width);
            let id = Id::new(("plugin_annotation", model.document, &set.key, index));
            // Its width is known after the first frame: right-aligned from then on.
            let width = ui.ctx().memory(|m| m.area_rect(id)).map_or(char_width * 20.0, |r| r.width());
            let left = (right - width).max(origin.x + row.rect.right() + char_width * 2.0).min(right - char_width * 4.0);
            let response = egui::Area::new(id)
                .order(egui::Order::Middle)
                .fixed_pos(egui::pos2(left, origin.y + row.rect.top()))
                .constrain(false)
                .show(ui.ctx(), |ui| {
                    ui.set_clip_rect(viewport);
                    egui::Frame::new()
                        .fill(blend(colors.background, color, 0.15))
                        .stroke(Stroke::new(1.0_f32, blend(colors.background, color, 0.6)))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(4, 0))
                        .show(ui, |ui| {
                            ui.set_max_width((right - left).max(char_width * 4.0));
                            show_html(ui, &annotation.html, html_colors, font_size)
                        })
                        .inner
                });
            if let Some(href) = response.inner {
                events.push(UiEvent::AnnotationLinkClicked { document: model.document, key: set.key.clone(), href });
            }
            next_right.insert(row_index, left - char_width);
        }
    }
}

/// A plugin's popup, below the char it points at (above it when there is no room).
#[allow(clippy::too_many_arguments)]
fn show_popup(
    ui: &egui::Ui,
    document: DocumentId,
    popup: &PopupViewModel,
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    font_size: f32,
    html_colors: &HtmlColors,
    colors: &Colors,
    events: &mut Vec<UiEvent>,
) {
    let at = galley.pos_from_ccursor(CCursor::new(popup.location)).translate(origin.to_vec2());
    if !viewport.intersects(at) {
        return;
    }
    let ctx = ui.ctx();
    let id = Id::new(("plugin_popup", document));
    // Its size last frame decides whether it fits below.
    let last_height = ctx.memory(|m| m.area_rect(id)).map_or(0.0, |r| r.height());
    let below = at.bottom() + 2.0 + last_height <= viewport.bottom() || at.top() - last_height - 2.0 < viewport.top();
    let (position, pivot) = if below {
        (egui::pos2(at.left(), at.bottom() + 2.0), Align2::LEFT_TOP)
    } else {
        (egui::pos2(at.left(), at.top() - 2.0), Align2::LEFT_BOTTOM)
    };
    let response = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .pivot(pivot)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(blend(colors.background, colors.foreground, 0.06))
                .stroke(Stroke::new(1.0_f32, colors.guide))
                .show(ui, |ui| {
                    ui.set_max_width(popup.max_width);
                    egui::ScrollArea::vertical()
                        .max_height(popup.max_height)
                        .auto_shrink([true, true])
                        .show(ui, |ui| show_html(ui, &popup.html, html_colors, font_size))
                        .inner
                })
                .inner
        });
    if let Some(href) = response.inner {
        events.push(UiEvent::PopupLinkClicked(href));
    }
    if popup.hide_on_mouse_move_away {
        let keep = response.response.rect.union(at).expand(24.0);
        if ctx.pointer_hover_pos().is_some_and(|p| !keep.contains(p)) {
            events.push(UiEvent::PopupDismissed);
        }
    }
}

/// Rows of the auto-complete list shown at once (it scrolls past them).
const AUTO_COMPLETE_ROWS: usize = 12;

/// The auto-complete list, below the typed word (above it when there is no room): a kind badge,
/// the trigger with the typed chars highlighted, the annotation at the right; the selected
/// completion's details under it. A click commits a row.
#[allow(clippy::too_many_arguments)]
fn show_auto_complete(
    ui: &egui::Ui,
    document: DocumentId,
    list: &AutoCompleteViewModel,
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    font: &FontId,
    row_height: f32,
    html_colors: &HtmlColors,
    colors: &Colors,
    events: &mut Vec<UiEvent>,
) {
    let at = galley.pos_from_ccursor(CCursor::new(list.location)).translate(origin.to_vec2());
    if !viewport.intersects(at) {
        return;
    }
    let ctx = ui.ctx();
    let id = Id::new(("auto_complete", document));
    let char_width = ui.fonts(|f| f.glyph_width(font, '0'));
    let longest = list.items.iter().map(|i| i.trigger.chars().count() + i.annotation.chars().count() + 6).max().unwrap_or(10);
    let width = (longest as f32 * char_width).clamp(char_width * 24.0, char_width * 70.0);
    let row = row_height + 2.0;
    let list_height = row * list.items.len().min(AUTO_COMPLETE_ROWS) as f32;
    let last_height = ctx.memory(|m| m.area_rect(id)).map_or(list_height, |r| r.height());
    let below = at.bottom() + 2.0 + last_height <= viewport.bottom() || at.top() - last_height - 2.0 < viewport.top();
    let (position, pivot) = if below {
        (egui::pos2(at.left(), at.bottom() + 2.0), Align2::LEFT_TOP)
    } else {
        (egui::pos2(at.left(), at.top() - 2.0), Align2::LEFT_BOTTOM)
    };
    let background = blend(colors.background, colors.foreground, 0.06);
    let accent = if colors.background.r() as u16 + colors.background.g() as u16 + colors.background.b() as u16 > 384 {
        Color32::from_rgb(0, 102, 204)
    } else {
        Color32::from_rgb(102, 217, 239)
    };
    // Scroll to the selected row only when it changes, so the wheel still scrolls the list.
    let scrolled_key = Id::new(("auto_complete_scrolled", document));
    let scroll_to_selected = ctx.data(|d| d.get_temp::<usize>(scrolled_key)) != Some(list.selected);
    ctx.data_mut(|d| d.insert_temp(scrolled_key, list.selected));

    let mut clicked = None;
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .pivot(pivot)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(background)
                .stroke(Stroke::new(1.0_f32, colors.guide))
                .inner_margin(egui::Margin::same(2))
                .show(ui, |ui| {
                    ui.set_width(width);
                    // Without a minimum, an area's scroll area keeps to egui's 64 px.
                    let scroll = egui::ScrollArea::vertical().max_height(list_height).min_scrolled_height(list_height);
                    scroll.auto_shrink([false, true]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (index, item) in list.items.iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(width, row), egui::Sense::CLICK);
                            if index == list.selected {
                                ui.painter().rect_filled(rect, 2.0, colors.selection);
                                if scroll_to_selected {
                                    ui.scroll_to_rect(rect, None);
                                }
                            } else if response.hovered() {
                                ui.painter().rect_filled(rect, 2.0, blend(background, colors.foreground, 0.06));
                            }
                            if response.clicked() {
                                clicked = Some(index);
                            }
                            paint_completion_row(ui, rect, item, font, char_width, colors, accent);
                        }
                    });
                    if !list.details.is_empty() {
                        ui.separator();
                        show_html(ui, &list.details_html, html_colors, font.size * 0.9);
                    }
                });
        });
    if let Some(index) = clicked {
        events.push(UiEvent::CompletionClicked(index));
    }
}

fn paint_completion_row(ui: &egui::Ui, rect: Rect, item: &CompletionViewModel, font: &FontId, char_width: f32, colors: &Colors, accent: Color32) {
    let painter = ui.painter().with_clip_rect(rect);
    let badge = Rect::from_min_size(rect.left_top() + egui::vec2(3.0, 2.0), egui::vec2(char_width * 1.6, rect.height() - 4.0));
    if !item.kind_letter.is_empty() {
        painter.rect_filled(badge, 3.0, kind_color(item.kind_id).gamma_multiply(0.85));
        painter.text(badge.center(), Align2::CENTER_CENTER, &item.kind_letter, font.clone(), Color32::BLACK);
    }
    let mut job = egui::text::LayoutJob::default();
    for (index, c) in item.trigger.chars().enumerate() {
        let matched = item.matched.contains(&index);
        let format = egui::TextFormat {
            font_id: font.clone(),
            color: if matched { accent } else { colors.foreground },
            underline: if matched { Stroke::new(1.0_f32, accent) } else { Stroke::NONE },
            ..Default::default()
        };
        job.append(c.encode_utf8(&mut [0; 4]), 0.0, format);
    }
    let trigger = ui.fonts(|f| f.layout_job(job));
    let text_left = badge.right() + char_width * 0.8;
    painter.galley(egui::pos2(text_left, rect.center().y - trigger.size().y / 2.0), trigger.clone(), colors.foreground);
    if !item.annotation.is_empty() {
        let left = text_left + trigger.size().x + char_width * 2.0;
        let annotation = ui.fonts(|f| f.layout_no_wrap(item.annotation.clone(), font.clone(), colors.gutter_foreground));
        let x = (rect.right() - annotation.size().x - char_width).max(left);
        painter.galley(egui::pos2(x, rect.center().y - annotation.size().y / 2.0), annotation, colors.gutter_foreground);
    }
}

/// The badge color of Sublime's `KindId`s (and its `KIND_ID_COLOR_*`).
fn kind_color(kind: u8) -> Color32 {
    match kind {
        1 | 16 => Color32::from_rgb(249, 38, 114),  // keyword: pinkish
        2 | 15 => Color32::from_rgb(174, 129, 255), // type: purplish
        3 | 10 => Color32::from_rgb(253, 151, 31),  // function: orangish
        4 | 14 => Color32::from_rgb(102, 153, 255), // namespace: bluish
        5 | 11 => Color32::from_rgb(230, 219, 116), // navigation: yellowish
        6 | 9 => Color32::from_rgb(240, 90, 80),    // markup: redish
        7 | 13 => Color32::from_rgb(102, 217, 239), // variable: cyanish
        8 | 12 => Color32::from_rgb(166, 226, 46),  // snippet: greenish
        _ => Color32::from_gray(170),
    }
}

/// When the mouse has rested over the text, the gutter or after a line for [`HOVER_DELAY`], says
/// where once (`on_hover`); nothing over a popup or panel on top.
#[allow(clippy::too_many_arguments)]
fn report_hover(
    ui: &egui::Ui,
    model: &EditorViewModel,
    phantoms: &[PhantomViewModel],
    galley: &egui::Galley,
    origin: egui::Pos2,
    viewport: Rect,
    gutter_width: f32,
    events: &mut Vec<UiEvent>,
) {
    let document = model.document;
    let ctx = ui.ctx();
    let key = Id::new(("hover_reported", document));
    let Some(pointer) = ctx.pointer_hover_pos().filter(|p| viewport.contains(*p)) else {
        ctx.data_mut(|d| d.remove::<(usize, u8)>(key));
        return;
    };
    // Over a phantom, the point is the phantom's (as over the text there).
    let phantom_point = phantoms
        .iter()
        .find(|p| ctx.memory(|m| m.area_rect(phantom_id(document, p.id))).is_some_and(|r| r.contains(pointer)))
        .map(|p| p.point);
    // Over an annotation, the point is its region's.
    let annotation_point = model.regions.iter().flat_map(|set| set.annotations.iter().enumerate().map(move |(i, a)| (set, i, a))).find_map(|(set, index, annotation)| {
        let id = Id::new(("plugin_annotation", document, &set.key, index));
        ctx.memory(|m| m.area_rect(id)).is_some_and(|r| r.contains(pointer)).then_some(annotation.point)
    });
    if phantom_point.is_none() && annotation_point.is_none() && ctx.layer_id_at(pointer) != Some(ui.layer_id()) {
        return;
    }
    let still = ctx.input(|i| i.pointer.time_since_last_movement());
    if still < HOVER_DELAY {
        ctx.data_mut(|d| d.remove::<(usize, u8)>(key));
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(HOVER_DELAY - still + 0.02));
        return;
    }
    let point = phantom_point
        .or(annotation_point)
        .unwrap_or_else(|| galley.cursor_from_pos(pointer - origin).ccursor.index);
    let row = galley.rows.iter().find(|r| pointer.y >= origin.y + r.rect.top() && pointer.y < origin.y + r.rect.bottom());
    let zone = if phantom_point.is_some() {
        HoverZone::Text
    } else if annotation_point.is_some() {
        HoverZone::Margin
    } else if pointer.x < viewport.left() + gutter_width {
        HoverZone::Gutter
    } else if row.is_some_and(|r| pointer.x > origin.x + r.rect.right()) {
        HoverZone::Margin
    } else {
        HoverZone::Text
    };
    let reported = (point, zone as u8);
    if ctx.data(|d| d.get_temp::<(usize, u8)>(key)) != Some(reported) {
        ctx.data_mut(|d| d.insert_temp(key, reported));
        events.push(UiEvent::Hover { document, point, zone });
    }
}

/// `a` moved towards `b` by `amount` (0 to 1).
fn blend(a: Color32, b: Color32, amount: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * amount).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
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
    minimap_border: Option<Color32>,
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
            minimap_border: s.minimap_border.map(to_color32),
        }
    }
}
