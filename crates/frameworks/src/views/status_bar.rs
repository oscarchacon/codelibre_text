//! Status bar: caret position, plugins' statuses and messages on the left; indentation, line endings, encoding and
//! syntax on the right (clickable, as in Sublime).

use egui::{RichText, Sense, Ui};
use entities::localization::Translations;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::StatusBarViewModel;

use super::theme::Chrome;

pub fn show(ui: &mut Ui, chrome: &Chrome, text: &Translations, status: &StatusBarViewModel, events: &mut Vec<UiEvent>) {
    let small = |s: &str| RichText::new(s).size(12.0).color(chrome.fg_dim);
    ui.horizontal_centered(|ui| {
        ui.add_space(8.0);
        ui.label(small(&status.position));
        for plugin_status in &status.plugin_statuses {
            ui.add_space(12.0);
            ui.label(small(plugin_status));
        }
        if let Some(message) = &status.message {
            ui.add_space(16.0);
            ui.label(RichText::new(message).size(12.0).color(chrome.fg));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            let link = |ui: &mut Ui, s: &str| ui.add(egui::Label::new(small(s)).sense(Sense::CLICK));
            if link(ui, &status.syntax).on_hover_text(text.tr("Set syntax")).clicked() {
                events.push(UiEvent::SyntaxClicked);
            }
            ui.add_space(14.0);
            ui.label(small(&status.encoding));
            ui.add_space(14.0);
            if link(ui, &status.line_ending).on_hover_text(text.tr("Toggle line endings")).clicked() {
                events.push(UiEvent::LineEndingClicked);
            }
            ui.add_space(14.0);
            if link(ui, &status.indentation).on_hover_text(text.tr("Toggle translate tabs to spaces")).clicked() {
                events.push(UiEvent::IndentationClicked);
            }
        });
    });
}
