//! Resolves key presses through the keymap and queues the bound commands.

use entities::keymap::{KeyCombo, KeyContext, KeyResult};
use entities::settings::Settings;
use serde_json::Value;
use use_cases::Editor;

use crate::view_state::ViewState;

/// Commands the text widget already does when it receives the key itself.
/// Bindings to them let the key through instead of running the command.
const WIDGET_COMMANDS: &[&str] = &[
    "undo", "redo", "redo_or_repeat", "soft_undo", "soft_redo", "copy", "cut", "paste",
    "select_all", "move", "move_to", "left_delete", "right_delete", "delete_word",
    "insert", "insert_best_completion", "commit_completion",
];

pub enum KeyHandling {
    /// The key belongs to the text / focused widget.
    PassThrough,
    /// The key was used by a binding (or a chord); the widgets must not see it.
    Consumed,
}

/// `alternatives`: equivalent readings of the key (logical and physical), see `Keymap::press`.
pub fn key_pressed(editor: &mut Editor, view: &mut ViewState, alternatives: &[KeyCombo]) -> KeyHandling {
    let context = ViewKeyContext {
        overlay_visible: view.quick_panel.is_some(),
        panel_visible: view.find_panel.visible,
        selection_empty: editor.active_document().is_none_or(|d| d.selection().is_empty()),
        settings: editor.view_settings(),
    };
    let was_waiting = editor.state().keymap.is_waiting_for_chord();
    let handling = match editor.press_key(alternatives, &context) {
        KeyResult::Unbound => KeyHandling::PassThrough,
        KeyResult::WaitingForChord => KeyHandling::Consumed,
        KeyResult::Command(command) if WIDGET_COMMANDS.contains(&command.name.as_str()) => KeyHandling::PassThrough,
        KeyResult::Command(command) => {
            view.queue_command(command);
            KeyHandling::Consumed
        }
    };

    let keymap = &editor.state().keymap;
    if keymap.is_waiting_for_chord() {
        let message = format!("{} was pressed. Waiting for second key…", keymap.pending_chord_display());
        editor.notify(message);
    } else if was_waiting {
        editor.clear_status();
    }
    handling
}

/// Answers keymap `context` conditions from the current screen state.
struct ViewKeyContext {
    overlay_visible: bool,
    panel_visible: bool,
    selection_empty: bool,
    settings: Settings,
}

impl KeyContext for ViewKeyContext {
    fn query(&self, key: &str, operator: &str, operand: &Value) -> Option<bool> {
        let value = match key {
            "overlay_visible" | "overlay_has_focus" => Value::Bool(self.overlay_visible),
            "panel_visible" | "panel_has_focus" => Value::Bool(self.panel_visible),
            "selection_empty" => Value::Bool(self.selection_empty),
            "num_selections" => Value::from(1),
            "auto_complete_visible" | "has_next_field" | "has_prev_field" | "is_recording" | "popup_visible" => {
                Value::Bool(false)
            }
            k if k.starts_with("setting.") => self.settings.get(&k["setting.".len()..]).cloned().unwrap_or_default(),
            _ => return None,
        };
        match operator {
            "equal" => Some(value == *operand),
            "not_equal" => Some(value != *operand),
            _ => None,
        }
    }
}
