//! The output panel at the bottom (`show_panel {"panel": "output.<name>"}`): a build's or a
//! plugin's output, read-only, following its end as text arrives. Esc or × closes it (`hide_panel`).
//! Double-clicking a result opens its file; the result visited last is highlighted. A right click
//! opens the text's context menu (`Context.sublime-menu`), whose commands run on the panel's view.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, LayoutJob};
use egui::{Align, Color32, FontId, Galley, Id, RichText, Sense, TextFormat, Ui};
use entities::command::Command;
use entities::menu::{ContextMenuTarget, CONSOLE_INPUT};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{ContextMenuViewModel, OutputPanelViewModel};

use super::context_menu;
use super::theme::Chrome;
use crate::input;
use crate::syntax_engine::SharedSyntaxEngine;

/// The panel's text in its syntax's colors (see [`OutputPanelViewModel::highlight`]), from where
/// the panel starts showing it; None without a syntax.
pub fn highlighted(engine: &SharedSyntaxEngine, panel: &OutputPanelViewModel) -> Option<LayoutJob> {
    let highlight = panel.highlight.as_ref()?;
    let font = FontId::monospace(panel.font_size);
    let mut engine = engine.borrow_mut();
    let whole = engine.layout_job(
        highlight.document,
        &highlight.full_text,
        &highlight.syntax,
        &highlight.color_scheme,
        font,
    );
    let start = highlight.start;
    let sections = whole
        .sections
        .into_iter()
        .filter(|section| section.byte_range.end > start)
        .map(|mut section| {
            section.byte_range =
                section.byte_range.start.max(start) - start..section.byte_range.end - start;
            section
        })
        .collect();
    Some(LayoutJob {
        text: whole.text[start..].to_string(),
        sections,
        ..Default::default()
    })
}

/// `highlighted`: the text already colored by [`highlighted`] (else it is drawn in one color).
pub fn show(
    ui: &mut Ui,
    chrome: &Chrome,
    panel: &OutputPanelViewModel,
    highlighted: Option<&LayoutJob>,
    menu: Option<&ContextMenuViewModel>,
    events: &mut Vec<UiEvent>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&panel.name).color(chrome.fg_dim).size(11.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = ui.add(
                egui::Label::new(RichText::new("×").size(15.0).color(chrome.fg_dim))
                    .sense(Sense::CLICK),
            );
            if close.clicked() {
                events.push(UiEvent::RunCommand(Command::new("hide_panel")));
            }
        });
    });
    if let Some((input, _)) = &panel.console_input {
        egui::TopBottomPanel::bottom(Id::new(("console_input_row", panel.id)))
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 2)))
            .show_separator_line(false)
            .show_inside(ui, |ui| console_input(ui, chrome, input, menu, events));
    }
    let scroll = if panel.word_wrap {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    };
    scroll
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let galley = lay_out(ui, chrome, panel, highlighted);
            let text = ui.add(
                egui::Label::new(galley.clone())
                    .selectable(false)
                    .sense(Sense::CLICK),
            );
            if panel.take_focus {
                text.request_focus();
            }
            if text.clicked() {
                events.push(UiEvent::OutputPanelFocused(panel.id));
            }
            if let Some(caret) = panel.caret {
                let at = galley.pos_from_ccursor(CCursor::new(caret));
                let origin = text.rect.min.to_vec2();
                ui.painter().line_segment(
                    [at.left_top() + origin, at.left_bottom() + origin],
                    egui::Stroke::new(1.0_f32, chrome.fg),
                );
            }
            // A right click: the context menu of the char clicked, in the whole text.
            let menu_point = Id::new(("output_panel_menu_point", panel.id));
            if text.secondary_clicked() {
                if let Some(pointer) = text.interact_pointer_pos() {
                    let at = galley
                        .cursor_from_pos(pointer - text.rect.min)
                        .ccursor
                        .index;
                    let keys = input::modifier_names(ui.input(|i| i.modifiers));
                    ui.data_mut(|d| d.insert_temp(menu_point, (panel.first_char + at, keys)));
                }
            }
            let (point, modifier_keys) = ui
                .data(|d| d.get_temp::<(usize, Vec<String>)>(menu_point))
                .unwrap_or((panel.first_char, Vec::new()));
            let target = ContextMenuTarget::Text {
                document: panel.id,
                point,
                modifier_keys,
            };
            context_menu::attach(&text, target, menu, events);
            if text.double_clicked() {
                if let Some(pointer) = text.interact_pointer_pos() {
                    let cursor = galley.cursor_from_pos(pointer - text.rect.min);
                    events.push(UiEvent::OutputPanelLineOpened(
                        panel.first_line + cursor.pcursor.paragraph,
                    ));
                }
            }
            scroll_to_new_result(ui, panel, &galley, text.rect.min);
        });
    // The console's field takes the focus when the console shows: asked after the panel's text,
    // which asks for it too, so the field's request is the one kept.
    if let Some((_, true)) = &panel.console_input {
        ui.ctx()
            .memory_mut(|m| m.request_focus(Id::new(CONSOLE_INPUT)));
    }
}

/// The console's input field: Python, run on Enter; Up / Down go through the lines run before.
/// Its right click opens `Console Input Widget Context.sublime-menu`.
fn console_input(
    ui: &mut Ui,
    chrome: &Chrome,
    input: &str,
    menu: Option<&ContextMenuViewModel>,
    events: &mut Vec<UiEvent>,
) {
    let id = Id::new(CONSOLE_INPUT);
    let before_right_press = context_menu::state_before_right_press(ui.ctx(), id);
    let mut text = input.to_string();
    let field = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(f32::INFINITY)
            .text_color(chrome.fg)
            .background_color(chrome.panel_bg)
            .hint_text(">>>")
            .font(egui::TextStyle::Monospace),
    );
    context_menu::attach_to_field(&field, CONSOLE_INPUT, menu, events);
    context_menu::keep_selection(ui.ctx(), id, &field, before_right_press);
    if text != input {
        events.push(UiEvent::ConsoleInputChanged(text));
    }
    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        events.push(UiEvent::ConsoleInputEntered);
        // At once: the text area, drawn after, would take the focus and this Enter.
        field.request_focus();
    } else if field.has_focus() {
        let (up, down) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
            )
        });
        if up || down {
            events.push(UiEvent::ConsoleHistory(up));
        }
    }
}

fn lay_out(
    ui: &Ui,
    chrome: &Chrome,
    panel: &OutputPanelViewModel,
    highlighted: Option<&LayoutJob>,
) -> Arc<Galley> {
    let mut job = highlighted.cloned().unwrap_or_else(|| {
        let mut job = LayoutJob::default();
        job.append(
            &panel.text,
            0.0,
            TextFormat::simple(FontId::monospace(panel.font_size), chrome.fg),
        );
        job
    });
    job.wrap.max_width = if panel.word_wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    if let Some((start, end)) = panel
        .current_line
        .and_then(|line| line_range(&panel.text, line))
    {
        paint_background(&mut job, start..end, chrome.hover_bg);
    }
    let selection = chrome.accent.gamma_multiply(0.35);
    for range in &panel.selections {
        paint_background(&mut job, range.clone(), selection);
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// Gives the bytes `range` of the job's text a background, splitting the sections at its edges.
pub fn paint_background(job: &mut LayoutJob, range: Range<usize>, color: Color32) {
    let mut sections = Vec::with_capacity(job.sections.len() + 2);
    for section in job.sections.drain(..) {
        let bytes = section.byte_range.clone();
        let (inside_start, inside_end) = (bytes.start.max(range.start), bytes.end.min(range.end));
        if inside_start >= inside_end {
            sections.push(section);
            continue;
        }
        let part = |byte_range: Range<usize>, background: Option<Color32>| {
            let mut part = section.clone();
            part.byte_range = byte_range;
            if let Some(color) = background {
                part.format.background = color;
            }
            part
        };
        if bytes.start < inside_start {
            sections.push(part(bytes.start..inside_start, None));
        }
        sections.push(part(inside_start..inside_end, Some(color)));
        if inside_end < bytes.end {
            sections.push(part(inside_end..bytes.end, None));
        }
    }
    job.sections = sections;
}

/// The bytes of a line, without its line break.
fn line_range(text: &str, line: usize) -> Option<(usize, usize)> {
    let start = match line {
        0 => 0,
        n => text.match_indices('\n').nth(n - 1)?.0 + 1,
    };
    Some((
        start,
        text[start..]
            .find('\n')
            .map_or(text.len(), |length| start + length),
    ))
}

fn scroll_to_new_result(
    ui: &mut Ui,
    panel: &OutputPanelViewModel,
    galley: &Galley,
    origin: egui::Pos2,
) {
    let seen = Id::new("output_panel_result_revision");
    if ui.data(|data| data.get_temp::<u64>(seen)) == Some(panel.result_revision) {
        return;
    }
    ui.data_mut(|data| data.insert_temp(seen, panel.result_revision));
    if let Some((start, _)) = panel
        .current_line
        .and_then(|line| line_range(&panel.text, line))
    {
        let at = galley.pos_from_ccursor(CCursor::new(panel.text[..start].chars().count()));
        ui.scroll_to_rect(at.translate(origin.to_vec2()), Some(Align::Center));
    }
}
