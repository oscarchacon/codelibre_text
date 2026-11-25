//! Tests for `interface_adapters::controllers::mouse_controller`, with the built-in mousemaps.

use entities::command::Command;
use entities::mousemap::{MouseModifiers, Mousemap};
use interface_adapters::controllers::mouse_controller::{
    press_action, MouseAction, MouseButton, MousePress,
};
use interface_adapters::gateways::builtin_resources::MOUSEMAPS;
use interface_adapters::gateways::sublime_resources::add_mouse_bindings;
use serde_json::json;
use use_cases::editing::{DragBy, DragCombine};

use DragBy::*;
use DragCombine::*;
use MouseButton::*;

fn mousemap(file: &str) -> Mousemap {
    let (_, text) = MOUSEMAPS
        .iter()
        .find(|(name, _)| name.contains(file))
        .unwrap();
    let mut mousemap = Mousemap::default();
    add_mouse_bindings(&mut mousemap, text).unwrap();
    mousemap
}

fn press(button: MouseButton, keys: &str, clicks: u32) -> MousePress {
    MousePress {
        button,
        modifiers: MouseModifiers::parse(keys.split_whitespace()).unwrap(),
        clicks,
    }
}

/// The selections a press makes, `None` when the widget makes them (or nothing does).
fn drag(
    map: &Mousemap,
    button: MouseButton,
    keys: &str,
    clicks: u32,
) -> Option<(DragBy, DragCombine)> {
    press_action(map, press(button, keys, clicks)).and_then(|a| a.drag)
}

#[test]
fn windows_and_linux_follow_sublimes_mousemaps() {
    for platform in ["Windows", "Linux"] {
        let map = mousemap(platform);
        let gesture = |button, keys, clicks| drag(&map, button, keys, clicks);
        assert_eq!(gesture(Primary, "", 1), None);
        assert_eq!(gesture(Primary, "", 2), None);
        assert_eq!(gesture(Primary, "shift", 1), None);
        assert_eq!(gesture(Primary, "ctrl", 1), Some((Characters, Add)));
        assert_eq!(gesture(Primary, "ctrl", 2), Some((Words, Add)));
        assert_eq!(gesture(Primary, "ctrl", 3), Some((Lines, Add)));
        assert_eq!(gesture(Primary, "ctrl", 5), Some((Lines, Add)));
        assert_eq!(gesture(Primary, "alt", 1), Some((Characters, Subtract)));
        assert_eq!(gesture(Primary, "alt", 2), Some((Words, Subtract)));
        assert_eq!(gesture(Primary, "shift alt", 1), Some((Columns, Replace)));
        assert_eq!(gesture(Primary, "shift alt", 2), Some((Columns, Replace)));
        assert_eq!(gesture(Primary, "ctrl shift alt", 1), Some((Columns, Add)));
        assert_eq!(gesture(Primary, "ctrl shift", 1), None);
        assert_eq!(gesture(Secondary, "shift", 1), Some((Columns, Replace)));
        assert_eq!(gesture(Secondary, "ctrl shift", 1), Some((Columns, Add)));
        assert_eq!(
            gesture(Secondary, "alt shift", 1),
            Some((Columns, Subtract))
        );
        // The right click's `context_menu` is the view's own.
        assert_eq!(press_action(&map, press(Secondary, "", 1)), None);
    }
    let windows = mousemap("Windows");
    assert_eq!(drag(&windows, Middle, "", 1), Some((Columns, Replace)));
    assert_eq!(drag(&windows, Middle, "ctrl", 1), Some((Columns, Add)));
    assert_eq!(drag(&windows, Middle, "alt", 1), Some((Columns, Subtract)));
    assert_eq!(press_action(&mousemap("Linux"), press(Middle, "", 1)), None);
}

#[test]
fn macos_follows_sublimes_mousemap() {
    let map = mousemap("OSX");
    let gesture = |button, keys, clicks| drag(&map, button, keys, clicks);
    assert_eq!(gesture(Primary, "", 1), None);
    assert_eq!(gesture(Primary, "super", 1), Some((Characters, Add)));
    assert_eq!(gesture(Primary, "super", 2), Some((Words, Add)));
    assert_eq!(gesture(Primary, "super", 3), Some((Lines, Add)));
    assert_eq!(
        gesture(Primary, "shift super", 1),
        Some((Characters, Subtract))
    );
    assert_eq!(gesture(Primary, "alt", 1), Some((Columns, Replace)));
    assert_eq!(gesture(Primary, "alt super", 1), Some((Columns, Add)));
    assert_eq!(
        gesture(Primary, "alt shift super", 1),
        Some((Columns, Subtract))
    );
    assert_eq!(gesture(Middle, "", 1), Some((Columns, Replace)));
    assert_eq!(gesture(Middle, "super", 1), Some((Columns, Add)));
    assert_eq!(gesture(Secondary, "shift", 1), None);
    // Ctrl + click opens the context menu there: the view's own.
    assert_eq!(press_action(&map, press(Primary, "ctrl", 1)), None);
}

#[test]
fn the_side_buttons_switch_tabs_on_release() {
    for platform in ["Windows", "Linux", "OSX"] {
        let map = mousemap(platform);
        let release = |button| press_action(&map, press(button, "", 1)).and_then(|a| a.release);
        assert_eq!(release(Back), Some(Command::new("prev_view")));
        assert_eq!(release(Forward), Some(Command::new("next_view")));
    }
}

#[test]
fn later_bindings_override_earlier_ones() {
    let mut map = mousemap("Windows");
    // A user's mousemap: Ctrl + click goes to the definition (the click puts the caret first).
    add_mouse_bindings(
        &mut map,
        r#"[
            // Comments and trailing commas, as Sublime's files have them.
            { "button": "button1", "count": 1, "modifiers": ["ctrl"],
              "press_command": "drag_select", "command": "goto_definition" },
            { "button": "button3", "press_command": "paste_selection_clipboard" },
            // Chords (another button held) and contexts are left out.
            { "button": "button1", "modifiers": ["button2"], "command": "select_all" },
            { "button": "button2", "press_command": "scroll_drag", "context": [{"key": "x"}] },
        ]"#,
    )
    .unwrap();
    assert_eq!(
        press_action(&map, press(Primary, "ctrl", 1)),
        Some(MouseAction {
            drag: None,
            press: None,
            release: Some(Command::new("goto_definition")),
        })
    );
    // Its double click is still the default's.
    assert_eq!(drag(&map, Primary, "ctrl", 2), Some((Words, Add)));
    assert_eq!(
        press_action(&map, press(Middle, "", 1)),
        Some(MouseAction {
            drag: None,
            press: Some(Command::new("paste_selection_clipboard")),
            release: None,
        })
    );
    assert_eq!(press_action(&map, press(Primary, "", 1)), None);
    assert_eq!(press_action(&map, press(Secondary, "", 1)), None);
}

#[test]
fn drag_select_arguments() {
    let mut map = Mousemap::default();
    add_mouse_bindings(
        &mut map,
        &json!([
            { "button": "button1", "modifiers": ["alt"], "press_command": "drag_select",
              "press_args": { "by": "words", "additive": true } },
            { "button": "button1", "modifiers": ["ctrl", "shift"], "press_command": "drag_select",
              "press_args": { "additive": true, "extend": true } },
            { "button": "button1", "modifiers": ["super"], "press_command": "drag_select",
              "press_args": { "by": "columns" } },
        ])
        .to_string(),
    )
    .unwrap();
    assert_eq!(drag(&map, Primary, "alt", 1), Some((Words, Add)));
    // Extending is the widget's (Shift + click), even when adding.
    assert_eq!(press_action(&map, press(Primary, "ctrl shift", 1)), None);
    assert_eq!(drag(&map, Primary, "super", 1), Some((Columns, Replace)));
    // Exactly the binding's modifiers.
    assert_eq!(drag(&map, Primary, "super shift", 1), None);
}
