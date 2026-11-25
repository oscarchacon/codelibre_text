//! Mouse bindings (Sublime's `.sublime-mousemap`): a button, the modifier keys held and how many
//! clicks in a row, bound to a command run when the button goes down (`press_command`) and one
//! run when it goes up (`command`). Later bindings override earlier ones, as in Sublime.

use crate::command::Command;

/// The modifier keys of a binding or a press, by Sublime's names (`super` is Cmd on macOS).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl MouseModifiers {
    /// A binding's `"modifiers"` list. `None` for a name that is no key: Sublime's chords hold
    /// another button (`"button2"`), which presses here never do.
    pub fn parse<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut modifiers = Self::default();
        for name in names {
            match name.trim().to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" | "cmd" | "command" | "meta" => modifiers.super_key = true,
                // `primary` is ctrl on Windows/Linux and cmd on macOS.
                "primary" if cfg!(target_os = "macos") => modifiers.super_key = true,
                "primary" => modifiers.ctrl = true,
                _ => return None,
            }
        }
        Some(modifiers)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MouseBinding {
    /// Sublime's name: `button1` (left), `button2` (right), `button3` (middle), `button4` /
    /// `button5` (back / forward), `scroll_up`, `scroll_down`.
    pub button: String,
    /// Clicks in a row (2: a double click); `None` matches any.
    pub count: Option<u32>,
    pub modifiers: MouseModifiers,
    /// Run when the button goes down (`drag_select` follows the pointer while it is down).
    pub press_command: Option<Command>,
    /// Run when the button goes up.
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mousemap {
    pub bindings: Vec<MouseBinding>,
}

impl Mousemap {
    pub fn add(&mut self, binding: MouseBinding) {
        self.bindings.push(binding);
    }

    /// The binding of a press: the last one with its button and exactly its modifiers whose count
    /// is the press's (clicks past three count as three, as Sublime binds no more) or any.
    pub fn find(
        &self,
        button: &str,
        modifiers: MouseModifiers,
        clicks: u32,
    ) -> Option<&MouseBinding> {
        let clicks = clicks.clamp(1, 3);
        self.bindings.iter().rev().find(|b| {
            b.button == button
                && b.modifiers == modifiers
                && b.count.is_none_or(|count| count == clicks)
        })
    }
}
