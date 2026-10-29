//! # Use Cases (second circle)
//!
//! What the application does: open, edit, find, save and close documents, manage folders,
//! settings and packages, restore the session. The [`Editor`] holds the application state and
//! each module adds the use cases of one area as methods on it.
//!
//! Use cases never touch the file system, the UI or a library directly. Everything external is
//! reached through the gateway traits in [`gateways`], which the outer circles implement.
//!
//! | Module | Use cases |
//! |---|---|
//! | [`documents`] | new, open, switch, close, reopen, revert, syntax, line endings |
//! | [`saving`] | save, save as, save all |
//! | [`editing`] | text commands, go to line, paste |
//! | [`find_replace`] | find next/previous, find under caret, replace |
//! | [`preferences`] | settings, font size, color scheme, edit settings files |
//! | [`packages`] | load / reload Sublime resources |
//! | [`localization`] | interface language, translated messages |
//! | [`help`] | open a link, report an issue |
//! | [`plugins`] | load and run Python plugins, the console |
//! | [`plugin_api`] | the `sublime_api` functions plugins call |
//! | [`plugin_panels`] | the quick and input panels plugins open |
//! | [`plugin_hooks`] | key presses (`on_query_context`) and commands (`on_text_command`, …) plugins take part in |
//! | [`output_panels`] | output panels (build and plugin output at the bottom), the `append` command |
//! | [`project`] | folders, folder listing, file index |
//! | [`search`] | command palette, Goto Anything, pickers |
//! | [`session`] | hot exit: save and restore the session, exit confirmation |

pub mod documents;
pub mod editing;
pub mod editor;
pub mod find_replace;
pub mod gateways;
pub mod help;
pub mod localization;
pub mod output_panels;
pub mod packages;
pub mod plugin_api;
pub mod plugin_hooks;
pub mod plugin_panels;
pub mod plugins;
pub mod preferences;
pub mod project;
pub mod saving;
pub mod search;
pub mod session;

pub use editor::{Editor, EditorState, Gateways};
