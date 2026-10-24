//! # Interface Adapters (third circle)
//!
//! Converts between the use cases and the outside world:
//!
//! - [`controllers`] turn user input (keys, menu clicks, UI events) into use case calls.
//! - [`presenters`] turn the application state into [`view_models`]: plain data the views draw.
//! - [`view_state`] is presentation-only state (which panels are open, which folders are expanded).
//! - [`gateways`] implement the use case gateways that are about *file formats*: Sublime Text's
//!   `.sublime-settings`, `.sublime-keymap`, `.sublime-menu`, `.sublime-commands`,
//!   `.sublime-color-scheme` and the session file.
//!
//! Nothing here knows which UI toolkit draws the views.

pub mod controllers;
pub mod gateways;
pub mod presenters;
pub mod view_models;
pub mod view_state;
