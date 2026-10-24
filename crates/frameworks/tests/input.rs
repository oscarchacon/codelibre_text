//! Tests for `input` (egui keys → keymap key combos).

use codelibre_text::input::key_combos;
use egui::{Key, Modifiers};
use entities::keymap::KeyCombo;

fn parse(spec: &str) -> KeyCombo {
    KeyCombo::parse(spec).unwrap()
}

#[test]
fn letters_with_modifiers() {
    assert_eq!(key_combos(Key::P, None, Modifiers::CTRL | Modifiers::SHIFT), vec![parse("ctrl+shift+p")]);
    assert_eq!(key_combos(Key::Num1, None, Modifiers::ALT), vec![parse("alt+1")]);
    assert_eq!(key_combos(Key::A, None, Modifiers::NONE), vec![parse("a")]);
}

#[test]
fn the_command_modifier_is_ctrl_outside_macos() {
    let combo = &key_combos(Key::S, None, Modifiers::COMMAND)[0];
    if cfg!(target_os = "macos") {
        assert!(!combo.ctrl);
    } else {
        assert!(combo.ctrl);
    }
}

#[test]
fn the_mac_command_key_is_super() {
    let modifiers = Modifiers { mac_cmd: true, ..Default::default() };
    assert!(key_combos(Key::K, None, modifiers)[0].super_key);
}

#[test]
fn named_keys_use_sublime_names() {
    for (key, name) in [
        (Key::F3, "f3"),
        (Key::Escape, "escape"),
        (Key::Enter, "enter"),
        (Key::PageDown, "pagedown"),
        (Key::ArrowUp, "up"),
        (Key::Slash, "/"),
        (Key::OpenBracket, "["),
        (Key::Equals, "="),
        (Key::Minus, "-"),
        (Key::Backtick, "`"),
    ] {
        assert_eq!(key_combos(key, None, Modifiers::NONE)[0].key, name, "{key:?}");
    }
}

#[test]
fn the_physical_key_is_an_alternative_reading() {
    let combos = key_combos(Key::Questionmark, Some(Key::Slash), Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(combos, vec![parse("ctrl+shift+?"), parse("ctrl+shift+/")]);
}

#[test]
fn keys_without_a_sublime_name_are_dropped() {
    assert!(key_combos(Key::F20, None, Modifiers::NONE).is_empty());
    assert_eq!(key_combos(Key::F20, Some(Key::F2), Modifiers::NONE), vec![parse("f2")]);
}
