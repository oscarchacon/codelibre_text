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
//! | [`clones`] | several views of one buffer (`clone_file`) |
//! | [`windows`] | several windows (`new_window`), each with its views, layout and panels |
//! | [`groups`] | the window's layout, focusing a group, moving views between groups |
//! | [`saving`] | save, save as, save all |
//! | [`editing`] | text commands, go to line, paste |
//! | [`clipboard_history`] | what was copied, for Paste from History |
//! | [`snippets`] | `insert_snippet` and visiting its fields with Tab |
//! | [`completions`] | auto-complete: the list, its sources (plugins, completions files, snippets, words), Tab completion |
//! | [`find_replace`] | find next/previous, find under caret, replace |
//! | [`preferences`] | settings, font size, color scheme, edit settings files |
//! | [`packages`] | load / reload Sublime resources |
//! | [`localization`] | interface language, translated messages |
//! | [`help`] | open a link, report an issue |
//! | [`plugins`] | load and run Python plugins, the console |
//! | [`plugin_api`] | the `sublime_api` functions plugins call |
//! | [`plugin_panels`] | the quick and input panels plugins open |
//! | [`plugin_popups`] | popups and phantoms plugins show (minihtml), their links, `on_hover` |
//! | [`plugin_hooks`] | key presses (`on_query_context`) and commands (`on_text_command`, …) plugins take part in |
//! | [`html_export`] | `view.export_to_html`: the text as HTML with the color scheme's styles |
//! | [`output_panels`] | output panels (build and plugin output at the bottom), the `append` command, `next_result` |
//! | [`builds`] | build systems: `build`, `exec`, `cancel_build` |
//! | [`project`] | projects (`.sublime-project`), folders, folder listing, file index |
//! | [`search`] | command palette, Goto Anything, pickers |
//! | [`session`] | hot exit: save and restore the session, exit confirmation |

pub mod builds;
pub mod clipboard_history;
pub mod clones;
pub mod completions;
pub mod context_menus;
pub mod documents;
pub mod editing;
pub mod editor;
pub mod find_replace;
pub mod gateways;
pub mod groups;
pub mod help;
pub mod html_export;
pub mod localization;
pub mod output_panels;
pub mod packages;
pub mod plugin_api;
pub mod plugin_commands;
pub mod plugin_hooks;
pub mod plugin_panels;
pub mod plugin_popups;
pub mod plugins;
pub mod preferences;
pub mod project;
pub mod saving;
pub mod scopes;
pub mod search;
pub mod session;
pub mod side_bar;
pub mod snippets;
pub mod windows;

pub use editor::{Editor, EditorState, Gateways};
