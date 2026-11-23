//! Mouse gestures in a view's text that make several selections: which `drag_select` a press
//! starts, as Sublime's default mousemaps (`Default (Windows|Linux|OSX).sublime-mousemap`) bind
//! them. A plain click (or Shift + click) is the text widget's own.

use use_cases::editing::{DragBy, DragCombine};

/// The systems whose mousemaps differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePlatform {
    Windows,
    Linux,
    MacOs,
}

impl MousePlatform {
    /// The system this was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Secondary,
    Middle,
}

/// A button going down in the text: which, the modifier keys held (`command` is Ctrl, Cmd on
/// macOS) and how many clicks in a row it is (2: a double click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePress {
    pub button: MouseButton,
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
    pub clicks: u32,
}

/// The `drag_select` a press starts, None when it is not one of these gestures:
///
/// | Gesture | Linux / Windows | macOS |
/// |---|---|---|
/// | Add a caret / selection | Ctrl + click / drag | Cmd + click / drag |
/// | Take one away | Alt + click / drag | Shift + Cmd + click / drag |
/// | Add a word / line | Ctrl + double / triple click | Cmd + double / triple click |
/// | Column | Shift + Alt + drag, Shift + right drag; middle drag (Windows) | Alt + drag, middle drag |
/// | Add a column | Ctrl + Shift + Alt + drag, Ctrl + Shift + right drag; Ctrl + middle drag (Windows) | Cmd + Alt + drag, Cmd + middle drag |
pub fn drag_gesture(press: MousePress, platform: MousePlatform) -> Option<(DragBy, DragCombine)> {
    let by_clicks = match press.clicks {
        0 | 1 => DragBy::Characters,
        2 => DragBy::Words,
        _ => DragBy::Lines,
    };
    let MousePress {
        button,
        shift,
        alt,
        command,
        ..
    } = press;
    let columns = |add: bool| {
        Some((
            DragBy::Columns,
            if add {
                DragCombine::Add
            } else {
                DragCombine::Replace
            },
        ))
    };
    match platform {
        MousePlatform::MacOs => match (button, shift, alt, command) {
            (MouseButton::Primary, false, false, true) => Some((by_clicks, DragCombine::Add)),
            (MouseButton::Primary, true, false, true) => Some((by_clicks, DragCombine::Subtract)),
            (MouseButton::Primary, false, true, add) => columns(add),
            (MouseButton::Middle, false, false, add) => columns(add),
            _ => None,
        },
        _ => match (button, shift, alt, command) {
            (MouseButton::Primary, false, false, true) => Some((by_clicks, DragCombine::Add)),
            (MouseButton::Primary, false, true, false) => Some((by_clicks, DragCombine::Subtract)),
            (MouseButton::Primary, true, true, add) => columns(add),
            (MouseButton::Secondary, true, false, add) => columns(add),
            (MouseButton::Middle, false, false, add) if platform == MousePlatform::Windows => {
                columns(add)
            }
            _ => None,
        },
    }
}
