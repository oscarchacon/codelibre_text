//! Resolves key presses through the keymap and queues the bound commands.

use entities::command::Command;
use entities::keymap::{compare_text, KeyCombo, KeyContext, KeyResult};
use entities::selector::matches_selector;
use entities::settings::Settings;
use serde_json::Value;
use use_cases::scopes::CaretContext;
use use_cases::Editor;

use crate::view_state::ViewState;

/// Commands the text widget already does when it receives the key itself.
/// Bindings to them let the key through instead of running the command.
const WIDGET_COMMANDS: &[&str] = &[
    "undo", "redo", "redo_or_repeat", "soft_undo", "soft_redo", "copy", "cut", "paste",
    "select_all", "move", "move_to", "left_delete", "right_delete",
    "insert",
];

/// Keys that move in the auto-complete list while it shows, and by how many rows.
const AUTO_COMPLETE_KEYS: &[(&str, isize)] = &[("up", -1), ("down", 1), ("pageup", -8), ("pagedown", 8)];

/// Whether the text widget does a [`WIDGET_COMMANDS`] binding's command when it gets the key:
/// it moves by characters, lines, words and pages and to the ends of lines and of the text, not
/// to brackets, by sub-words or by Sublime's `stops`, which then run as commands.
fn widget_does(command: &Command) -> bool {
    match command.name.as_str() {
        "move" => matches!(command.str_arg("by"), "characters" | "lines" | "words" | "word_ends" | "pages"),
        "move_to" => matches!(command.str_arg("to"), "bol" | "eol" | "hardbol" | "hardeol" | "bof" | "eof"),
        _ => true,
    }
}

/// The find / input panel or the quick panel is open and the text does not have the focus.
fn typing_goes_to_a_panel(view: &ViewState) -> bool {
    !view.text_has_focus && (view.find_panel.visible || view.input_panel.is_some() || view.quick_panel.is_some())
}

/// A key that types a symbol (`)`, `"`, `]`), with no modifier but Shift.
fn typed_character(alternatives: &[KeyCombo]) -> bool {
    alternatives.iter().any(|c| c.is_symbol() && !c.ctrl && !c.alt && !c.super_key)
}

pub enum KeyHandling {
    /// The key belongs to the text / focused widget.
    PassThrough,
    /// The key was used by a binding (or a chord); the widgets must not see it.
    Consumed,
}

/// `alternatives`: equivalent readings of the key (logical and physical), see `Keymap::press`.
pub fn key_pressed(editor: &mut Editor, view: &mut ViewState, alternatives: &[KeyCombo]) -> KeyHandling {
    // Esc closes a plugin's popup first, as in Sublime.
    if editor.plugin_popup().is_some() && KeyCombo::parse("escape").is_some_and(|escape| alternatives.contains(&escape)) {
        editor.hide_plugin_popup();
        return KeyHandling::Consumed;
    }
    // The auto-complete list takes the arrow keys while it shows (Enter, Tab and Esc are bindings).
    if editor.auto_complete_visible() && view.text_has_focus {
        let moved = AUTO_COMPLETE_KEYS.iter().find(|(name, _)| KeyCombo::parse(name).is_some_and(|combo| alternatives.contains(&combo)));
        if let Some(&(_, rows)) = moved {
            editor.move_auto_complete_selection(rows);
            return KeyHandling::Consumed;
        }
    }
    let context = ViewKeyContext {
        auto_complete_visible: editor.auto_complete_visible(),
        overlay_visible: view.quick_panel.is_some(),
        panel_visible: view.find_panel.visible || view.input_panel.is_some() || editor.shown_output_panel().is_some(),
        // Output panels are not focused: the caret stays in the text.
        panel_has_focus: view.find_panel.visible || view.input_panel.is_some(),
        num_selections: editor.active_document().map_or(1, |d| d.extra_selections().len() + 1),
        settings: editor.view_settings(),
        // The scopes come from the highlighting cache. Not while a panel takes the typing: `(` in
        // the find panel must not pair brackets in the file.
        carets: if typing_goes_to_a_panel(view) { Vec::new() } else { editor.caret_contexts() },
        has_next_field: editor.has_snippet_field(true),
        has_prev_field: editor.has_snippet_field(false),
        // Typing is not a command here: what Tab completed counts only while nothing changed.
        last_command: if editor.last_command_was_completion() {
            Some("insert_best_completion".to_string())
        } else {
            view.last_command.clone().filter(|c| c != "insert_best_completion")
        },
    };
    let was_waiting = editor.state().keymap.is_waiting_for_chord();
    let hooked = hooks_widget_edits(editor, view);
    let handling = match editor.press_key(alternatives, &context) {
        KeyResult::Unbound => KeyHandling::PassThrough,
        KeyResult::WaitingForChord => KeyHandling::Consumed,
        // Listeners hear every widget command as a command (and may replace it): leaving these
        // keys to the widget would hide undo, deleting, caret movement and the clipboard from them.
        // (Whole-line copy/paste is decided once, on the clipboard event: see `clipboard_shortcut`.)
        // A typed character bound to one of them (`)` stepping over the closing bracket with `move`)
        // runs it: the widget would type the character instead.
        KeyResult::Command(command)
            if !hooked && WIDGET_COMMANDS.contains(&command.name.as_str()) && widget_does(&command) && !typed_character(alternatives) =>
        {
            KeyHandling::PassThrough
        }
        KeyResult::Command(command) => {
            view.queue_command(command);
            KeyHandling::Consumed
        }
    };

    let keymap = &editor.state().keymap;
    if keymap.is_waiting_for_chord() {
        let message = editor.tr_args("{keys} was pressed. Waiting for second key…", &[("keys", &keymap.pending_chord_display())]);
        editor.notify(message);
    } else if was_waiting {
        editor.clear_status();
    }
    handling
}

/// A widget edit in the text goes through the commands when plugins listen to them. Other fields
/// keep their keys: the command would edit the text, not the find panel.
fn hooks_widget_edits(editor: &Editor, view: &ViewState) -> bool {
    view.text_has_focus && editor.plugins_hear_text_commands()
}

/// The system's copy / cut / paste shortcut (`command` is `copy`, `cut` or `paste`). In the text,
/// copy and cut always run as commands (the clipboard history keeps what they copy; the widget
/// still copies, a frame later); paste does when it is a whole line (see below) or plugins listen
/// to text commands. Then the event must not reach the widget (returns `Consumed`).
pub fn clipboard_shortcut(editor: &mut Editor, view: &mut ViewState, command: &str) -> KeyHandling {
    // The second key of a chord (Ctrl+K, Ctrl+V: Paste from History) is the key, not the event.
    if editor.state().keymap.is_waiting_for_chord() {
        let key = match command {
            "copy" => "c",
            "cut" => "x",
            _ => "v",
        };
        let modifier = if cfg!(target_os = "macos") { "super" } else { "ctrl" };
        if let Some(combo) = KeyCombo::parse(&format!("{modifier}+{key}")) {
            return key_pressed(editor, view, &[combo]);
        }
    }
    let copies = view.text_has_focus && command != "paste";
    if copies || takes_over_from_widget(editor, view, command) || hooks_widget_edits(editor, view) {
        view.queue_command(Command::new(command));
        KeyHandling::Consumed
    } else {
        KeyHandling::PassThrough
    }
}

/// Copy, cut and paste of a whole line (nothing selected, `copy_with_empty_selection`) are done by
/// the use cases, not the text widget, but only while the text has the focus.
fn takes_over_from_widget(editor: &mut Editor, view: &ViewState, command: &str) -> bool {
    view.text_has_focus
        && match command {
            "copy" | "cut" => editor.copies_whole_line(),
            "paste" => editor.pastes_whole_line(),
            _ => false,
        }
}

/// Answers keymap `context` conditions from the current screen state.
struct ViewKeyContext {
    auto_complete_visible: bool,
    overlay_visible: bool,
    panel_visible: bool,
    panel_has_focus: bool,
    num_selections: usize,
    settings: Settings,
    /// Every selection of the active view: what `selector`, `selection_empty`,
    /// `preceding_text`, `following_text` and `text` look at. One of them must match, or all of
    /// them with `match_all`, as in Sublime.
    carets: Vec<CaretContext>,
    /// A snippet's next / previous field is there for Tab / Shift+Tab.
    has_next_field: bool,
    has_prev_field: bool,
    last_command: Option<String>,
}

impl ViewKeyContext {
    /// A condition asked of each selection: true when one answers true (all, with `match_all`).
    fn per_caret(&self, match_all: bool, answer: impl Fn(&CaretContext) -> Option<bool>) -> Option<bool> {
        if self.carets.is_empty() {
            return None;
        }
        let answers: Option<Vec<bool>> = self.carets.iter().map(answer).collect();
        let answers = answers?;
        Some(if match_all { answers.iter().all(|a| *a) } else { answers.iter().any(|a| *a) })
    }
}

impl KeyContext for ViewKeyContext {
    fn query(&self, key: &str, operator: &str, operand: &Value) -> Option<bool> {
        self.query_all(key, operator, operand, false)
    }

    fn query_all(&self, key: &str, operator: &str, operand: &Value, match_all: bool) -> Option<bool> {
        let equal = |value: bool| match operator {
            "equal" => Some(value == operand.as_bool().unwrap_or(true)),
            "not_equal" => Some(value != operand.as_bool().unwrap_or(true)),
            _ => None,
        };
        match key {
            // `eol_selector`: the scope at the end of the caret's line (a string still open there).
            "selector" | "eol_selector" => {
                let selector = operand.as_str()?;
                let eol = key == "eol_selector";
                let matches = |c: &CaretContext| matches_selector(if eol { &c.eol_scope } else { &c.scope }, selector);
                return match operator {
                    "equal" => self.per_caret(match_all, |c| Some(matches(c))),
                    "not_equal" => self.per_caret(match_all, |c| Some(!matches(c))),
                    _ => None,
                };
            }
            "selection_empty" => return self.per_caret(match_all, |c| equal(c.text.is_empty())),
            "preceding_text" => return self.per_caret(match_all, |c| compare_text(operator, &c.preceding_text, operand)),
            "following_text" => return self.per_caret(match_all, |c| compare_text(operator, &c.following_text, operand)),
            "text" => return self.per_caret(match_all, |c| compare_text(operator, &c.text, operand)),
            _ => {}
        }
        let value = match key {
            "overlay_visible" | "overlay_has_focus" => Value::Bool(self.overlay_visible),
            "panel_visible" => Value::Bool(self.panel_visible),
            "panel_has_focus" => Value::Bool(self.panel_has_focus),
            "num_selections" => Value::from(self.num_selections),
            "auto_complete_visible" => Value::Bool(self.auto_complete_visible),
            "has_next_field" => Value::Bool(self.has_next_field),
            "has_prev_field" => Value::Bool(self.has_prev_field),
            "last_command" => self.last_command.clone().map_or(Value::Null, Value::String),
            "is_recording" | "popup_visible" => Value::Bool(false),
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
