//! The one-line panel at the bottom that plugins open (`window.show_input_panel`). Esc closes it
//! through the keymap (`hide_panel`).

use egui::{Id, Key, RichText, Ui};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::InputPanelViewModel;

use super::theme::Chrome;

pub fn show(ui: &mut Ui, chrome: &Chrome, panel: &InputPanelViewModel, events: &mut Vec<UiEvent>) {
    let mut text = panel.text.clone();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if !panel.caption.is_empty() {
            ui.label(RichText::new(&panel.caption).color(chrome.fg_dim));
        }
        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(Id::new("input_panel_input"))
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if panel.take_focus {
            field.request_focus();
        }
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            events.push(UiEvent::InputPanelDone);
        }
    });
    ui.add_space(6.0);
    if text != panel.text {
        // Before a Done of the same frame, so it gets the final text.
        events.insert(0, UiEvent::InputPanelChanged(text));
    }
}
