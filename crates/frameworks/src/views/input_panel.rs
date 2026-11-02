//! The one-line panel at the bottom that plugins open (`window.show_input_panel`). Esc closes it
//! through the keymap (`hide_panel`). Its text is a document plugins can read and change, like
//! the editor view's: selections they set arrive by revision, the caret goes back as an event.

use egui::text::{CCursor, CCursorRange};
use egui::{Event, Id, Key, RichText, Ui};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::InputPanelViewModel;

use super::theme::Chrome;

pub fn show(ui: &mut Ui, chrome: &Chrome, panel: &InputPanelViewModel, events: &mut Vec<UiEvent>) {
    let mut text = panel.text.clone();
    let mut selected = None;
    let mut done = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if !panel.caption.is_empty() {
            ui.label(RichText::new(&panel.caption).color(chrome.fg_dim));
        }
        let id = Id::new("input_panel_input");
        let mut field = egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace)
            .show(ui);
        if panel.take_focus {
            field.response.request_focus();
        }
        if field.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            done = true;
            // The text area takes the focus back in this frame: it must not get these keys too.
            ui.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_))));
        }
        let seen = id.with("selection_revision");
        let revision = (panel.id, panel.selection_revision);
        if ui.data(|data| data.get_temp(seen)) != Some(revision) {
            ui.data_mut(|data| data.insert_temp(seen, revision));
            let (anchor, head) = panel.selection;
            field.state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(anchor), CCursor::new(head))));
            field.state.store(ui.ctx(), id);
        } else if let Some(range) = field.cursor_range {
            let range = range.as_ccursor_range();
            selected = Some((range.secondary.index, range.primary.index)).filter(|s| *s != panel.selection);
        }
    });
    ui.add_space(6.0);
    // Before a Done of the same frame, so it gets the final text; the caret after the text it is in.
    if let Some((anchor, head)) = selected {
        events.insert(0, UiEvent::InputPanelSelected { anchor, head });
    }
    if text != panel.text {
        events.insert(0, UiEvent::InputPanelChanged(text));
    }
    if done {
        events.push(UiEvent::InputPanelDone);
    }
}
