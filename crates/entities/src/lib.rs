//! # Entities (innermost circle)
//!
//! The editor's core business rules: what a document is, how text commands transform text and
//! selections, how find works, how key bindings are matched, how settings layer on top of each
//! other. Everything here is plain Rust: no file system, no UI, no syntax-highlighting engine.
//!
//! Nothing in this crate knows that the other circles exist.

pub mod build;
pub mod command;
pub mod completion;
pub mod document;
pub mod find;
pub mod keymap;
pub mod layout;
pub mod localization;
pub mod menu;
pub mod meta_info;
pub mod minihtml;
pub mod selection;
pub mod selector;
pub mod settings;
pub mod symbol;
pub mod text;
pub mod text_commands;
