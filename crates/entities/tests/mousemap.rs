//! Tests for `entities::mousemap`.

use entities::command::Command;
use entities::mousemap::{MouseBinding, MouseModifiers, Mousemap};

fn modifiers(names: &str) -> MouseModifiers {
    MouseModifiers::parse(names.split_whitespace()).unwrap()
}

fn binding(button: &str, keys: &str, count: Option<u32>, command: &str) -> MouseBinding {
    MouseBinding {
        button: button.into(),
        count,
        modifiers: modifiers(keys),
        press_command: None,
        command: Some(Command::new(command)),
    }
}

fn found(map: &Mousemap, button: &str, keys: &str, clicks: u32) -> Option<String> {
    map.find(button, modifiers(keys), clicks)
        .and_then(|b| b.command.as_ref())
        .map(|c| c.name.clone())
}

#[test]
fn modifiers_have_sublimes_names() {
    let all = modifiers("ctrl alt shift super");
    assert!(all.ctrl && all.alt && all.shift && all.super_key);
    assert_eq!(modifiers("control option cmd"), modifiers("ctrl alt super"));
    // A chord holds another button: no press matches it.
    assert_eq!(MouseModifiers::parse(["button2"]), None);
}

#[test]
fn the_last_binding_with_the_press_modifiers_and_count_wins() {
    let mut map = Mousemap::default();
    map.add(binding("button1", "ctrl", Some(1), "first"));
    map.add(binding("button1", "ctrl", Some(2), "double"));
    map.add(binding("button1", "ctrl", None, "any"));
    map.add(binding("button1", "ctrl", Some(1), "last"));
    map.add(binding("button1", "ctrl", Some(3), "triple"));
    assert_eq!(found(&map, "button1", "ctrl", 1).as_deref(), Some("last"));
    assert_eq!(found(&map, "button1", "ctrl", 2).as_deref(), Some("any"));
    assert_eq!(found(&map, "button1", "ctrl", 3).as_deref(), Some("triple"));
    // Clicks past three count as three.
    assert_eq!(found(&map, "button1", "ctrl", 7).as_deref(), Some("triple"));
    // Exactly its modifiers, and its button.
    assert_eq!(found(&map, "button1", "ctrl shift", 1), None);
    assert_eq!(found(&map, "button1", "", 1), None);
    assert_eq!(found(&map, "button2", "ctrl", 1), None);
}
