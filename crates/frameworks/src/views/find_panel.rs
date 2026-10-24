//! Find / replace panel at the bottom, with Sublime's option toggles.

use egui::{Id, Key, RichText, Ui};
use entities::find::FindOptions;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::FindPanelViewModel;

use super::theme::Chrome;

pub fn show(ui: &mut Ui, chrome: &Chrome, panel: &FindPanelViewModel, events: &mut Vec<UiEvent>) {
    let mut options = FindOptions {
        query: panel.query.clone(),
        regex: panel.regex,
        case_sensitive: panel.case_sensitive,
        whole_word: panel.whole_word,
        wrap: panel.wrap,
    };
    let mut replacement = panel.replacement.clone();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        toggle(ui, chrome, &mut options.regex, ".*", "Regular expression");
        toggle(ui, chrome, &mut options.case_sensitive, "Aa", "Case sensitive");
        toggle(ui, chrome, &mut options.whole_word, "“ ”", "Whole word");
        toggle(ui, chrome, &mut options.wrap, "⟲", "Wrap");
        ui.add_space(6.0);

        let query = ui.add(
            egui::TextEdit::singleline(&mut options.query)
                .id(Id::new("find_input"))
                .hint_text("Find")
                .desired_width((ui.available_width() - 260.0).max(160.0))
                .font(egui::TextStyle::Monospace),
        );
        if panel.focus_requested {
            query.request_focus();
        }
        if query.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let backwards = ui.input(|i| i.modifiers.shift);
            events.push(if backwards { UiEvent::FindPrevious } else { UiEvent::FindNext });
            query.request_focus();
        }

        let color = if panel.has_error { egui::Color32::from_rgb(240, 100, 90) } else { chrome.fg_dim };
        ui.label(RichText::new(&panel.status).size(11.5).color(color));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
                events.push(UiEvent::CloseFindPanel);
            }
            if ui.button("Find Prev").clicked() {
                events.push(UiEvent::FindPrevious);
            }
            if ui.button("Find").clicked() {
                events.push(UiEvent::FindNext);
            }
        });
    });

    if panel.replace_mode {
        ui.horizontal(|ui| {
            ui.add_space(4.0 * 32.0 + 6.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut replacement)
                    .id(Id::new("replace_input"))
                    .hint_text("Replace")
                    .desired_width((ui.available_width() - 260.0).max(160.0))
                    .font(egui::TextStyle::Monospace),
            );
            if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                events.push(UiEvent::ReplaceNext);
                field.request_focus();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(26.0);
                if ui.button("Replace All").clicked() {
                    events.push(UiEvent::ReplaceAll);
                }
                if ui.button("Replace").clicked() {
                    events.push(UiEvent::ReplaceNext);
                }
            });
        });
    }
    ui.add_space(6.0);

    let unchanged = options.query == panel.query
        && options.regex == panel.regex
        && options.case_sensitive == panel.case_sensitive
        && options.whole_word == panel.whole_word
        && options.wrap == panel.wrap;
    if !unchanged {
        // Applied before any Find/Replace click of this frame.
        events.insert(0, UiEvent::FindOptionsChanged(options));
    }
    if replacement != panel.replacement {
        events.insert(0, UiEvent::ReplacementChanged(replacement));
    }
}

fn toggle(ui: &mut Ui, chrome: &Chrome, value: &mut bool, label: &str, tooltip: &str) {
    let text = RichText::new(label).monospace().color(if *value { chrome.accent } else { chrome.fg_dim });
    let button = egui::Button::new(text).selected(*value).min_size(egui::vec2(28.0, 22.0));
    if ui.add(button).on_hover_text(tooltip).clicked() {
        *value = !*value;
    }
}
