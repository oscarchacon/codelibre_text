//! Controllers: translate input into use case calls.
//!
//! - [`command_controller`]: Sublime command names (`save`, `show_overlay`, …) from key bindings,
//!   menus and the command palette.
//! - [`keyboard_controller`]: key presses, resolved through the keymap.
//! - [`mouse_controller`]: the mouse gestures that make several selections.
//! - [`ui_controller`]: clicks and edits in the views ([`ui_controller::UiEvent`]).

pub mod command_controller;
pub mod keyboard_controller;
pub mod mouse_controller;
pub mod ui_controller;
