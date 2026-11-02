//! The output panel at the bottom (`show_panel {"panel": "output.<name>"}`): a build's or a
//! plugin's output, read-only, following its end as text arrives. Esc or × closes it (`hide_panel`).
//! Double-clicking a result opens its file; the result visited last is highlighted.

use std::sync::Arc;

use egui::text::{CCursor, LayoutJob};
use egui::{Align, FontId, Galley, Id, RichText, Sense, TextFormat, Ui};
use entities::command::Command;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::OutputPanelViewModel;

use super::theme::Chrome;

pub fn show(ui: &mut Ui, chrome: &Chrome, panel: &OutputPanelViewModel, events: &mut Vec<UiEvent>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&panel.name).color(chrome.fg_dim).size(11.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = ui.add(egui::Label::new(RichText::new("×").size(15.0).color(chrome.fg_dim)).sense(Sense::CLICK));
            if close.clicked() {
                events.push(UiEvent::RunCommand(Command::new("hide_panel")));
            }
        });
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let galley = lay_out(ui, chrome, panel);
            let text = ui.add(egui::Label::new(galley.clone()).selectable(false).sense(Sense::CLICK));
            if text.double_clicked() {
                if let Some(pointer) = text.interact_pointer_pos() {
                    let cursor = galley.cursor_from_pos(pointer - text.rect.min);
                    events.push(UiEvent::OutputPanelLineOpened(panel.first_line + cursor.pcursor.paragraph));
                }
            }
            scroll_to_new_result(ui, panel, &galley, text.rect.min);
        });
}

fn lay_out(ui: &Ui, chrome: &Chrome, panel: &OutputPanelViewModel) -> Arc<Galley> {
    let plain = TextFormat::simple(FontId::monospace(panel.font_size), chrome.fg);
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    match panel.current_line.and_then(|line| line_range(&panel.text, line)) {
        Some((start, end)) => {
            let current = TextFormat { background: chrome.hover_bg, ..plain.clone() };
            job.append(&panel.text[..start], 0.0, plain.clone());
            job.append(&panel.text[start..end], 0.0, current);
            job.append(&panel.text[end..], 0.0, plain);
        }
        None => job.append(&panel.text, 0.0, plain),
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// The bytes of a line, without its line break.
fn line_range(text: &str, line: usize) -> Option<(usize, usize)> {
    let start = match line {
        0 => 0,
        n => text.match_indices('\n').nth(n - 1)?.0 + 1,
    };
    Some((start, text[start..].find('\n').map_or(text.len(), |length| start + length)))
}

fn scroll_to_new_result(ui: &mut Ui, panel: &OutputPanelViewModel, galley: &Galley, origin: egui::Pos2) {
    let seen = Id::new("output_panel_result_revision");
    if ui.data(|data| data.get_temp::<u64>(seen)) == Some(panel.result_revision) {
        return;
    }
    ui.data_mut(|data| data.insert_temp(seen, panel.result_revision));
    if let Some((start, _)) = panel.current_line.and_then(|line| line_range(&panel.text, line)) {
        let at = galley.pos_from_ccursor(CCursor::new(panel.text[..start].chars().count()));
        ui.scroll_to_rect(at.translate(origin.to_vec2()), Some(Align::Center));
    }
}
