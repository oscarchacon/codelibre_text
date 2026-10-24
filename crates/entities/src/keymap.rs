//! Key bindings: key combinations in Sublime notation (`ctrl+shift+p`), chords (`["ctrl+k", "ctrl+b"]`)
//! and context conditions. Later bindings override earlier ones, as in Sublime.

use serde_json::Value;

use crate::command::Command;

/// One key press with modifiers, e.g. `ctrl+shift+p`. `key` uses Sublime's names
/// (`a`, `f3`, `enter`, `pageup`, `/`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub key: String,
}

impl KeyCombo {
    /// Parses Sublime notation such as `ctrl+shift+p`, `ctrl++` or `alt+keypad_plus`.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim().to_lowercase();
        let (modifiers, key) = if let Some(stripped) = spec.strip_suffix("++") {
            (stripped, "+")
        } else if spec == "+" {
            ("", "+")
        } else {
            spec.rsplit_once('+').unwrap_or(("", spec.as_str()))
        };
        let mut combo = KeyCombo {
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
            key: normalize_key_name(key),
        };
        for modifier in modifiers.split('+').filter(|m| !m.is_empty()) {
            match modifier {
                "ctrl" | "control" => combo.ctrl = true,
                "alt" | "option" => combo.alt = true,
                "shift" => combo.shift = true,
                "super" | "cmd" | "command" | "meta" => combo.super_key = true,
                // `primary` is ctrl on Windows/Linux and cmd on macOS.
                "primary" if cfg!(target_os = "macos") => combo.super_key = true,
                "primary" => combo.ctrl = true,
                _ => return None,
            }
        }
        (!combo.key.is_empty()).then_some(combo)
    }

    /// Human readable form for menus and the command palette, e.g. `Ctrl+Shift+P`.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.super_key {
            parts.push("Super".to_string());
        }
        let mut chars = self.key.chars();
        let key = match (chars.next(), self.key.chars().count()) {
            (Some(first), 1) => first.to_uppercase().collect(),
            (Some(first), _) => first.to_uppercase().collect::<String>() + chars.as_str(),
            (None, _) => String::new(),
        };
        parts.push(key);
        parts.join("+")
    }

    /// A single printable symbol such as `/` or `?`.
    fn is_symbol(&self) -> bool {
        self.key.chars().count() == 1 && !self.key.chars().all(char::is_alphanumeric)
    }
}

/// Normalizes Sublime's alternative key names to one spelling.
fn normalize_key_name(key: &str) -> String {
    match key {
        "plus" | "keypad_plus" => "+",
        "minus" | "keypad_minus" => "-",
        "equals" => "=",
        "page_up" | "prior" => "pageup",
        "page_down" | "next" => "pagedown",
        "return" | "keypad_enter" => "enter",
        "esc" => "escape",
        "del" => "delete",
        "ins" => "insert",
        "forward_slash" | "keypad_divide" => "/",
        "backquote" => "`",
        k if k.starts_with("keypad") && k.len() == 7 => &k[6..],
        k => k,
    }
    .to_string()
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub keys: Vec<KeyCombo>,
    pub command: Command,
    /// Sublime context conditions, e.g. `{"key": "overlay_visible", "operand": true}`.
    pub context: Vec<Value>,
}

impl Binding {
    pub fn display(&self) -> String {
        self.keys.iter().map(KeyCombo::display).collect::<Vec<_>>().join(", ")
    }
}

/// Answers keymap context questions about the current state of the editor.
/// Returning `None` means "unknown key", and the binding does not apply.
pub trait KeyContext {
    fn query(&self, key: &str, operator: &str, operand: &Value) -> Option<bool>;
}

pub enum KeyResult {
    /// No binding: the key goes to the text as usual.
    Unbound,
    /// The first key of a chord; waiting for the next one.
    WaitingForChord,
    Command(Command),
}

#[derive(Debug, Default)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
    pending_chord: Vec<KeyCombo>,
}

impl Keymap {
    pub fn add(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    pub fn is_waiting_for_chord(&self) -> bool {
        !self.pending_chord.is_empty()
    }

    pub fn pending_chord_display(&self) -> String {
        self.pending_chord.iter().map(KeyCombo::display).collect::<Vec<_>>().join(", ")
    }

    /// Feeds one key press. `alternatives` are equivalent readings of the same key, e.g. the
    /// character it produced and the physical key, for keyboard layouts where they differ.
    pub fn press(&mut self, alternatives: &[KeyCombo], context: &dyn KeyContext) -> KeyResult {
        let mut candidates = Vec::new();
        for combo in alternatives {
            // A shifted symbol (`?`) may be written without shift in the keymap.
            if combo.shift && combo.is_symbol() {
                candidates.push(KeyCombo { shift: false, ..combo.clone() });
            }
            candidates.push(combo.clone());
        }

        let was_waiting = self.is_waiting_for_chord();
        for combo in candidates {
            let mut sequence = self.pending_chord.clone();
            sequence.push(combo);

            let mut is_prefix = false;
            for binding in self.bindings.iter().rev() {
                let starts_with_sequence =
                    binding.keys.len() >= sequence.len() && binding.keys[..sequence.len()] == sequence[..];
                if !starts_with_sequence || !context_matches(&binding.context, context) {
                    continue;
                }
                if binding.keys.len() == sequence.len() {
                    self.pending_chord.clear();
                    return KeyResult::Command(binding.command.clone());
                }
                is_prefix = true;
            }
            if is_prefix {
                self.pending_chord = sequence;
                return KeyResult::WaitingForChord;
            }
        }

        self.pending_chord.clear();
        if was_waiting {
            // An unknown second key of a chord is swallowed, like in Sublime.
            KeyResult::WaitingForChord
        } else {
            KeyResult::Unbound
        }
    }

    /// The key binding shown next to a command in menus and the palette.
    pub fn binding_for(&self, command: &Command) -> Option<String> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.command.same_as(command))
            .map(Binding::display)
    }
}

fn context_matches(conditions: &[Value], context: &dyn KeyContext) -> bool {
    conditions.iter().all(|condition| {
        let Some(key) = condition.get("key").and_then(Value::as_str) else {
            return false;
        };
        let operator = condition.get("operator").and_then(Value::as_str).unwrap_or("equal");
        let operand = condition.get("operand").cloned().unwrap_or(Value::Bool(true));
        context.query(key, operator, &operand).unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoContext;
    impl KeyContext for NoContext {
        fn query(&self, _: &str, _: &str, _: &Value) -> Option<bool> {
            None
        }
    }

    fn bind(keymap: &mut Keymap, keys: &[&str], command: &str) {
        keymap.add(Binding {
            keys: keys.iter().map(|k| KeyCombo::parse(k).unwrap()).collect(),
            command: Command::new(command),
            context: Vec::new(),
        });
    }

    fn key(spec: &str) -> Vec<KeyCombo> {
        vec![KeyCombo::parse(spec).unwrap()]
    }

    #[test]
    fn parses_sublime_notation() {
        let combo = KeyCombo::parse("ctrl++").unwrap();
        assert!(combo.ctrl && combo.key == "+");
        assert_eq!(KeyCombo::parse("ctrl+shift+p").unwrap().display(), "Ctrl+Shift+P");
        assert_eq!(KeyCombo::parse("page_up").unwrap().key, "pageup");
    }

    #[test]
    fn resolves_chords() {
        let mut keymap = Keymap::default();
        bind(&mut keymap, &["ctrl+k", "ctrl+b"], "toggle_side_bar");
        assert!(matches!(keymap.press(&key("ctrl+k"), &NoContext), KeyResult::WaitingForChord));
        match keymap.press(&key("ctrl+b"), &NoContext) {
            KeyResult::Command(c) => assert_eq!(c.name, "toggle_side_bar"),
            _ => panic!("chord not resolved"),
        }
    }

    #[test]
    fn later_bindings_win() {
        let mut keymap = Keymap::default();
        bind(&mut keymap, &["ctrl+s"], "save");
        bind(&mut keymap, &["ctrl+s"], "save_all");
        match keymap.press(&key("ctrl+s"), &NoContext) {
            KeyResult::Command(c) => assert_eq!(c.name, "save_all"),
            _ => panic!("not bound"),
        }
    }

    #[test]
    fn bindings_with_unknown_context_do_not_apply() {
        let mut keymap = Keymap::default();
        keymap.add(Binding {
            keys: key("escape"),
            command: Command::new("hide_overlay"),
            context: vec![serde_json::json!({ "key": "overlay_visible" })],
        });
        assert!(matches!(keymap.press(&key("escape"), &NoContext), KeyResult::Unbound));
    }
}
