//! Mouse presses in a view's text, resolved through the mousemaps (`Default (<platform>).sublime-mousemap`
//! of our `Default` and of every package, `User` last): the `drag_select` a press starts when it
//! makes several selections, and the other commands bound to it. A plain click, Shift + click and a
//! plain double / triple click are the text widget's own (its caret, its IME).

use entities::command::Command;
use entities::mousemap::{MouseModifiers, Mousemap};
use use_cases::editing::{DragBy, DragCombine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Secondary,
    Middle,
    /// The side buttons (Sublime's `button4` and `button5`).
    Back,
    Forward,
}

impl MouseButton {
    /// Sublime's name of the button.
    fn name(self) -> &'static str {
        match self {
            Self::Primary => "button1",
            Self::Secondary => "button2",
            Self::Middle => "button3",
            Self::Back => "button4",
            Self::Forward => "button5",
        }
    }
}

/// A button going down in the text: which, the modifier keys held (Sublime's names: Cmd is
/// `super`) and how many clicks in a row it is (2: a double click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePress {
    pub button: MouseButton,
    pub modifiers: MouseModifiers,
    pub clicks: u32,
}

/// What a press does besides the text widget's own.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseAction {
    /// The selections a `drag_select` makes while the button is down; `None` when the widget
    /// does it (a plain or Shift + click) or the binding has none.
    pub drag: Option<(DragBy, DragCombine)>,
    /// A `press_command` other than `drag_select`, run at once.
    pub press: Option<Command>,
    /// The binding's `command`, run when the button goes up.
    pub release: Option<Command>,
}

/// The commands the views do themselves when a button goes down.
const VIEWS_OWN: &[&str] = &["context_menu"];

/// What `press` does by `mousemap`; `None` when it leaves it all to the text widget. The
/// defaults (`assets/Default/Default (<platform>).sublime-mousemap`):
///
/// | Gesture | Linux / Windows | macOS |
/// |---|---|---|
/// | Add a caret / selection | Ctrl + click / drag | Cmd + click / drag |
/// | Take one away | Alt + click / drag | Shift + Cmd + click / drag |
/// | Add a word / line | Ctrl + double / triple click | Cmd + double / triple click |
/// | Column | Shift + Alt + drag, Shift + right drag; middle drag (Windows) | Alt + drag, middle drag |
/// | Add a column | Ctrl + Shift + Alt + drag, Ctrl + Shift + right drag; Ctrl + middle drag (Windows) | Cmd + Alt + drag, Cmd + middle drag |
/// | Previous / next tab | Back / Forward button | Back / Forward button |
pub fn press_action(mousemap: &Mousemap, press: MousePress) -> Option<MouseAction> {
    let binding = mousemap.find(press.button.name(), press.modifiers, press.clicks)?;
    let theirs = |command: &Option<Command>| {
        command
            .clone()
            .filter(|c| !VIEWS_OWN.contains(&c.name.as_str()))
    };
    let (drag, press) = match &binding.press_command {
        Some(command) if command.name == "drag_select" => (drag_select(command), None),
        other => (None, theirs(other)),
    };
    let release = theirs(&binding.command);
    (drag.is_some() || press.is_some() || release.is_some()).then_some(MouseAction {
        drag,
        press,
        release,
    })
}

/// The selections of a `drag_select {by, additive, subtractive, extend}`, `None` for those the
/// widget makes: replacing them by characters, words or lines, and extending (Shift + click).
fn drag_select(command: &Command) -> Option<(DragBy, DragCombine)> {
    if command.bool_arg("extend", false) {
        return None;
    }
    let by = match command.str_arg("by") {
        "columns" => DragBy::Columns,
        "words" => DragBy::Words,
        "lines" => DragBy::Lines,
        _ => DragBy::Characters,
    };
    let combine = if command.bool_arg("additive", false) {
        DragCombine::Add
    } else if command.bool_arg("subtractive", false) {
        DragCombine::Subtract
    } else {
        DragCombine::Replace
    };
    (by == DragBy::Columns || combine != DragCombine::Replace).then_some((by, combine))
}
