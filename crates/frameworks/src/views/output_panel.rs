//! The output panel at the bottom (`show_panel {"panel": "output.<name>"}`): a build's or a
//! plugin's output, read-only, following its end as text arrives. Esc or × closes it (`hide_panel`).

use egui::{RichText, Sense, Ui};
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
            let text = RichText::new(&panel.text).monospace().size(panel.font_size).color(chrome.fg);
            ui.add(egui::Label::new(text).wrap());
        });
}
