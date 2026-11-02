//! Tests for `entities::settings` (Sublime's settings layering).

use entities::settings::SettingsLayers;
use serde_json::json;

#[test]
fn typed_getters_with_defaults() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({
        "flag": true, "size": 12.5, "count": 3, "name": "x",
        "list": ["a", 1, "b"], "rulers": [80, "x", 120]
    }));
    let settings = layers.global();
    assert!(settings.bool("flag", false));
    assert_eq!(settings.f32("size", 0.0), 12.5);
    assert_eq!(settings.usize("count", 0), 3);
    assert_eq!(settings.str("name"), Some("x"));
    assert_eq!(settings.strings("list"), vec!["a", "b"]);
    assert_eq!(settings.numbers("rulers"), vec![80, 120]);
    assert!(settings.bool("missing", true));
    assert_eq!(settings.str("missing"), None);
    assert!(settings.strings("missing").is_empty());
}

#[test]
fn later_preference_layers_win() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({ "tab_size": 4, "word_wrap": false }));
    layers.add_preferences(&json!({ "tab_size": 2 }));
    let settings = layers.global();
    assert_eq!(settings.usize("tab_size", 0), 2);
    assert!(!settings.bool("word_wrap", true));
}

#[test]
fn syntax_settings_only_apply_to_that_syntax() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({ "tab_size": 4 }));
    layers.add_syntax_layer("Python", json!({ "tab_size": 8 }));
    assert_eq!(layers.for_syntax("Python").usize("tab_size", 0), 8);
    assert_eq!(layers.for_syntax("Rust").usize("tab_size", 0), 4);
    assert_eq!(layers.global().usize("tab_size", 0), 4);
}

#[test]
fn overrides_beat_every_layer() {
    let mut layers = SettingsLayers::default();
    layers.add_syntax_layer("Python", json!({ "tab_size": 8 }));
    layers.set_override("tab_size", json!(3));
    assert_eq!(layers.for_syntax("Python").usize("tab_size", 0), 3);
}

#[test]
fn toggle_override_flips_the_current_value() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({ "word_wrap": true }));
    layers.toggle_override("word_wrap");
    assert!(!layers.global().bool("word_wrap", true));
    layers.toggle_override("word_wrap");
    assert!(layers.global().bool("word_wrap", false));
}

#[test]
fn non_object_layers_are_ignored() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!([1, 2]));
    assert!(layers.global().get("0").is_none());
}

#[test]
fn values_with_a_view_s_own_settings_on_top() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({"tab_size": 4, "word_wrap": false}));
    let own = json!({"tab_size": 2}).as_object().unwrap().clone();
    let settings = layers.global().with(&own);
    assert_eq!(settings.usize("tab_size", 0), 2);
    assert_eq!(settings.values().len(), 2);
}

#[test]
fn removing_an_override_brings_back_the_preference() {
    let mut layers = SettingsLayers::default();
    layers.add_preferences(&json!({"tab_size": 4}));
    layers.set_override("tab_size", json!(8));
    layers.remove_override("tab_size");
    assert_eq!(layers.global().usize("tab_size", 0), 4);
}
