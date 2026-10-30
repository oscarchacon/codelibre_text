//! Tests for `entities::keymap` (Sublime key bindings).

use std::collections::HashMap;

use entities::command::Command;
use entities::keymap::*;
use serde_json::{json, Value};

/// Answers context queries from a fixed table; unknown keys return `None`.
#[derive(Default)]
struct Context(HashMap<&'static str, Value>);

impl KeyContext for Context {
    fn query(&self, key: &str, operator: &str, operand: &Value) -> Option<bool> {
        let value = self.0.get(key)?;
        match operator {
            "equal" => Some(value == operand),
            "not_equal" => Some(value != operand),
            _ => None,
        }
    }
}

fn key(spec: &str) -> KeyCombo {
    KeyCombo::parse(spec).unwrap_or_else(|| panic!("invalid key {spec}"))
}

fn binding(keys: &[&str], command: Command) -> Binding {
    Binding { keys: keys.iter().map(|k| key(k)).collect(), command, context: Vec::new() }
}

fn press(keymap: &mut Keymap, spec: &str) -> KeyResult {
    keymap.press(&[key(spec)], &Context::default())
}

fn command_name(result: KeyResult) -> String {
    match result {
        KeyResult::Command(c) => c.name,
        KeyResult::Unbound => "<unbound>".into(),
        KeyResult::WaitingForChord => "<chord>".into(),
    }
}

// ----- parsing and display -------------------------------------------------------------------------

#[test]
fn parses_modifiers_and_keys() {
    let combo = key("ctrl+alt+shift+super+a");
    assert!(combo.ctrl && combo.alt && combo.shift && combo.super_key);
    assert_eq!(combo.key, "a");
}

#[test]
fn parsing_ignores_case_and_whitespace() {
    assert_eq!(key(" CTRL+S "), key("ctrl+s"));
}

#[test]
fn plus_can_be_the_key() {
    let combo = key("ctrl++");
    assert!(combo.ctrl);
    assert_eq!(combo.key, "+");
    assert_eq!(key("+").key, "+");
}

#[test]
fn alternative_key_names_are_normalized() {
    assert_eq!(key("keypad_plus").key, "+");
    assert_eq!(key("keypad5").key, "5");
    assert_eq!(key("page_up").key, "pageup");
    assert_eq!(key("esc").key, "escape");
    assert_eq!(key("forward_slash").key, "/");
}

#[test]
fn primary_is_the_platform_command_key() {
    let combo = key("primary+s");
    if cfg!(target_os = "macos") {
        assert!(combo.super_key && !combo.ctrl);
    } else {
        assert!(combo.ctrl && !combo.super_key);
    }
}

#[test]
fn unknown_modifiers_and_empty_specs_are_invalid() {
    assert!(KeyCombo::parse("hyper+x").is_none());
    assert!(KeyCombo::parse("").is_none());
}

#[test]
fn display_is_human_readable() {
    assert_eq!(key("ctrl+shift+p").display(), "Ctrl+Shift+P");
    assert_eq!(key("f11").display(), "F11");
    assert_eq!(key("alt+pageup").display(), "Alt+Pageup");
    let chord = binding(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar"));
    assert_eq!(chord.display(), "Ctrl+K, Ctrl+B");
}

// ----- resolving key presses -----------------------------------------------------------------------

#[test]
fn unbound_keys_pass_through() {
    let mut keymap = Keymap::default();
    assert_eq!(command_name(press(&mut keymap, "ctrl+q")), "<unbound>");
}

#[test]
fn single_key_bindings_run_their_command() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+s"], Command::new("save")));
    assert_eq!(command_name(press(&mut keymap, "ctrl+s")), "save");
}

#[test]
fn later_bindings_win() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+s"], Command::new("save")));
    keymap.add(binding(&["ctrl+s"], Command::new("save_all")));
    assert_eq!(command_name(press(&mut keymap, "ctrl+s")), "save_all");
}

#[test]
fn chords_wait_for_the_second_key() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar")));
    assert_eq!(command_name(press(&mut keymap, "ctrl+k")), "<chord>");
    assert!(keymap.is_waiting_for_chord());
    assert_eq!(keymap.pending_chord_display(), "Ctrl+K");
    assert_eq!(command_name(press(&mut keymap, "ctrl+b")), "toggle_side_bar");
    assert!(!keymap.is_waiting_for_chord());
}

#[test]
fn a_wrong_second_key_cancels_the_chord_and_is_swallowed() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar")));
    keymap.add(binding(&["x"], Command::new("insert")));
    press(&mut keymap, "ctrl+k");
    assert_eq!(command_name(press(&mut keymap, "ctrl+z")), "<chord>");
    assert!(!keymap.is_waiting_for_chord());
    assert_eq!(command_name(press(&mut keymap, "x")), "insert");
}

#[test]
fn bindings_keep_their_arguments() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+g"], Command::with_args("show_overlay", json!({ "overlay": "goto", "text": ":" }))));
    match press(&mut keymap, "ctrl+g") {
        KeyResult::Command(c) => assert_eq!(c.str_arg("text"), ":"),
        _ => panic!("not bound"),
    }
}

#[test]
fn a_shifted_symbol_matches_a_binding_written_without_shift() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+/"], Command::new("toggle_comment")));
    let typed = KeyCombo { ctrl: true, alt: false, shift: true, super_key: false, key: "/".into() };
    assert_eq!(command_name(keymap.press(&[typed], &Context::default())), "toggle_comment");
}

#[test]
fn any_alternative_reading_of_the_key_can_match() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+z"], Command::new("undo")));
    let result = keymap.press(&[key("ctrl+y"), key("ctrl+z")], &Context::default());
    assert_eq!(command_name(result), "undo");
}

// ----- contexts ------------------------------------------------------------------------------------

fn escape_binding() -> Binding {
    Binding {
        keys: vec![key("escape")],
        command: Command::new("hide_overlay"),
        context: vec![json!({ "key": "overlay_visible", "operator": "equal", "operand": true })],
    }
}

#[test]
fn context_conditions_must_hold() {
    let mut keymap = Keymap::default();
    keymap.add(escape_binding());
    let visible = Context(HashMap::from([("overlay_visible", json!(true))]));
    let hidden = Context(HashMap::from([("overlay_visible", json!(false))]));
    assert_eq!(command_name(keymap.press(&[key("escape")], &visible)), "hide_overlay");
    assert_eq!(command_name(keymap.press(&[key("escape")], &hidden)), "<unbound>");
}

#[test]
fn unknown_context_keys_disable_the_binding() {
    let mut keymap = Keymap::default();
    keymap.add(escape_binding());
    assert_eq!(command_name(press(&mut keymap, "escape")), "<unbound>");
}

#[test]
fn context_defaults_are_equal_to_true() {
    let mut keymap = Keymap::default();
    keymap.add(Binding {
        keys: vec![key("tab")],
        command: Command::new("next_field"),
        context: vec![json!({ "key": "has_next_field" })],
    });
    let context = Context(HashMap::from([("has_next_field", json!(true))]));
    assert_eq!(command_name(keymap.press(&[key("tab")], &context)), "next_field");
}

#[test]
fn not_equal_operator() {
    let mut keymap = Keymap::default();
    keymap.add(Binding {
        keys: vec![key("enter")],
        command: Command::new("commit"),
        context: vec![json!({ "key": "setting.mode", "operator": "not_equal", "operand": "vi" })],
    });
    let context = Context(HashMap::from([("setting.mode", json!("normal"))]));
    assert_eq!(command_name(keymap.press(&[key("enter")], &context)), "commit");
}

// ----- shortcuts for menus and the palette ---------------------------------------------------------

#[test]
fn binding_for_finds_the_shortcut_of_a_command() {
    let mut keymap = Keymap::default();
    keymap.add(binding(&["ctrl+s"], Command::new("save")));
    keymap.add(binding(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar")));
    keymap.add(binding(&["ctrl+f"], Command::with_args("show_panel", json!({ "panel": "find" }))));
    keymap.add(binding(&["ctrl+h"], Command::with_args("show_panel", json!({ "panel": "replace" }))));

    assert_eq!(keymap.binding_for(&Command::with_args("save", json!({}))), Some("Ctrl+S".into()));
    assert_eq!(keymap.binding_for(&Command::new("toggle_side_bar")), Some("Ctrl+K, Ctrl+B".into()));
    assert_eq!(
        keymap.binding_for(&Command::with_args("show_panel", json!({ "panel": "replace" }))),
        Some("Ctrl+H".into())
    );
    assert_eq!(keymap.binding_for(&Command::new("exit")), None);
}

#[test]
fn a_conditions_match_all_reaches_the_context() {
    use std::cell::Cell;

    struct Recording(Cell<Option<bool>>);

    impl KeyContext for Recording {
        fn query(&self, _: &str, _: &str, _: &Value) -> Option<bool> {
            Some(true)
        }

        fn query_all(&self, _: &str, _: &str, _: &Value, match_all: bool) -> Option<bool> {
            self.0.set(Some(match_all));
            Some(true)
        }
    }

    let mut keymap = Keymap::default();
    keymap.add(Binding {
        keys: vec![KeyCombo::parse("f1").unwrap()],
        command: Command::new("x"),
        context: vec![serde_json::json!({ "key": "k", "match_all": true })],
    });
    let context = Recording(Cell::new(None));
    keymap.press(&[KeyCombo::parse("f1").unwrap()], &context);
    assert_eq!(context.0.get(), Some(true));
}
