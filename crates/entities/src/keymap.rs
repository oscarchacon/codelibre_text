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

    /// Human readable form for menus and the command palette, e.g. `Ctrl+Shift+P`
    /// (`⇧+⌘+P` on macOS).
    pub fn display(&self) -> String {
        self.display_for(cfg!(target_os = "macos"))
    }

    /// [`display`](Self::display) as the system writes it: modifier names joined by `+`, or on
    /// macOS its symbols in its order (⌃⌥⇧⌘), with symbols for keys such as ↩ ⌫ ↑ too.
    pub fn display_for(&self, macos: bool) -> String {
        let modifiers = if macos {
            [(self.ctrl, "⌃"), (self.alt, "⌥"), (self.shift, "⇧"), (self.super_key, "⌘")]
        } else {
            [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.super_key, "Super")]
        };
        let mut parts: Vec<String> = modifiers.iter().filter(|(on, _)| *on).map(|(_, name)| name.to_string()).collect();
        let symbol = match self.key.as_str() {
            _ if !macos => None,
            "enter" => Some("↩"),
            "tab" => Some("⇥"),
            "escape" => Some("⎋"),
            "backspace" => Some("⌫"),
            "delete" => Some("⌦"),
            "up" => Some("↑"),
            "down" => Some("↓"),
            "left" => Some("←"),
            "right" => Some("→"),
            "pageup" => Some("⇞"),
            "pagedown" => Some("⇟"),
            "home" => Some("↖"),
            "end" => Some("↘"),
            _ => None,
        };
        parts.push(symbol.map_or_else(|| self.key_name(), str::to_string));
        parts.join("+")
    }

    /// The key with a capital first letter (`P`, `F11`, `Pageup`).
    fn key_name(&self) -> String {
        let mut chars = self.key.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
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

    /// Like [`query`](Self::query), with the condition's `match_all`: whether every selection must
    /// match, not just one. Only plugins' contexts tell the difference.
    fn query_all(&self, key: &str, operator: &str, operand: &Value, _match_all: bool) -> Option<bool> {
        self.query(key, operator, operand)
    }
}

/// A context condition on a text (`preceding_text`, `following_text`, `text`): Sublime's
/// `equal`, `not_equal`, `regex_match` (the whole text), `not_regex_match`, `regex_contains`,
/// `not_regex_contains`. `None` for another operator or a regex that does not compile.
pub fn compare_text(operator: &str, text: &str, operand: &Value) -> Option<bool> {
    let pattern = operand.as_str()?;
    let regex = |anchored: bool| {
        let source = if anchored { format!("^(?:{pattern})$") } else { pattern.to_string() };
        regex::Regex::new(&source).ok()
    };
    match operator {
        "equal" => Some(text == pattern),
        "not_equal" => Some(text != pattern),
        "regex_match" => Some(regex(true)?.is_match(text)),
        "not_regex_match" => Some(!regex(true)?.is_match(text)),
        "regex_contains" => Some(regex(false)?.is_match(text)),
        "not_regex_contains" => Some(!regex(false)?.is_match(text)),
        _ => None,
    }
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
        self.binding(command).map(Binding::display)
    }

    /// The key of [`binding_for`](Self::binding_for)'s binding, when it is one key and not a chord.
    pub fn key_for(&self, command: &Command) -> Option<&KeyCombo> {
        match self.binding(command)?.keys.as_slice() {
            [key] => Some(key),
            _ => None,
        }
    }

    fn binding(&self, command: &Command) -> Option<&Binding> {
        self.bindings.iter().rev().find(|b| b.command.same_as(command))
    }
}

fn context_matches(conditions: &[Value], context: &dyn KeyContext) -> bool {
    conditions.iter().all(|condition| {
        let Some(key) = condition.get("key").and_then(Value::as_str) else {
            return false;
        };
        let operator = condition.get("operator").and_then(Value::as_str).unwrap_or("equal");
        let operand = condition.get("operand").cloned().unwrap_or(Value::Bool(true));
        let match_all = condition.get("match_all").and_then(Value::as_bool).unwrap_or(false);
        context.query_all(key, operator, &operand, match_all).unwrap_or(false)
    })
}
