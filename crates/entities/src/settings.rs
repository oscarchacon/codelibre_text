//! Settings with Sublime's layering: defaults, then `Preferences.sublime-settings` from every
//! package (`User` last), then per-syntax files such as `Python.sublime-settings`, then runtime
//! overrides (`toggle_setting`, which are not saved).

use std::collections::HashMap;

use serde_json::{Map, Value};

/// One resolved set of settings.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    values: Map<String, Value>,
}

impl Settings {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn f32(&self, key: &str, default: f32) -> f32 {
        self.get(key).and_then(Value::as_f64).map(|v| v as f32).unwrap_or(default)
    }

    pub fn usize(&self, key: &str, default: usize) -> usize {
        self.get(key).and_then(Value::as_u64).map(|v| v as usize).unwrap_or(default)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn strings(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    pub fn numbers(&self, key: &str) -> Vec<usize> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect())
            .unwrap_or_default()
    }

    fn merge(&mut self, layer: &Value) {
        if let Value::Object(map) = layer {
            self.values.extend(map.clone());
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SettingsLayers {
    preferences: Settings,
    by_syntax: HashMap<String, Vec<Value>>,
    overrides: Map<String, Value>,
}

impl SettingsLayers {
    /// Adds a `Preferences.sublime-settings` layer (call in load order).
    pub fn add_preferences(&mut self, layer: &Value) {
        self.preferences.merge(layer);
    }

    /// Adds a `<Syntax>.sublime-settings` layer.
    pub fn add_syntax_layer(&mut self, syntax: &str, layer: Value) {
        self.by_syntax.entry(syntax.to_string()).or_default().push(layer);
    }

    /// Settings for a view showing a document with the given syntax.
    pub fn for_syntax(&self, syntax: &str) -> Settings {
        let mut settings = self.preferences.clone();
        for layer in self.by_syntax.get(syntax).into_iter().flatten() {
            settings.merge(layer);
        }
        settings.merge(&Value::Object(self.overrides.clone()));
        settings
    }

    /// Settings that do not depend on a syntax.
    pub fn global(&self) -> Settings {
        self.for_syntax("")
    }

    pub fn set_override(&mut self, key: &str, value: Value) {
        self.overrides.insert(key.to_string(), value);
    }

    pub fn toggle_override(&mut self, key: &str) {
        let current = self.global().bool(key, false);
        self.set_override(key, Value::Bool(!current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn later_layers_and_syntax_settings_win() {
        let mut layers = SettingsLayers::default();
        layers.add_preferences(&json!({ "tab_size": 4, "word_wrap": false }));
        layers.add_preferences(&json!({ "tab_size": 2 }));
        layers.add_syntax_layer("Python", json!({ "tab_size": 8 }));
        assert_eq!(layers.global().usize("tab_size", 0), 2);
        assert_eq!(layers.for_syntax("Python").usize("tab_size", 0), 8);
        layers.toggle_override("word_wrap");
        assert!(layers.for_syntax("Python").bool("word_wrap", false));
    }
}
