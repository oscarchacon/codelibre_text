//! The main menu, the context menus and command palette entries.

use std::path::PathBuf;

use serde_json::Value;

use crate::command::Command;
use crate::document::DocumentId;

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub caption: String,
    /// Packages extend menus by reusing an `id` (e.g. `"tools"`).
    pub id: Option<String>,
    pub command: Option<Command>,
    /// Shows a check mark tied to the boolean setting in the command's `setting` argument.
    pub checkbox: bool,
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    pub fn is_separator(&self) -> bool {
        self.caption == "-"
    }
}

/// Merges a package's menu into the menu built so far: items with a known `id` add their
/// children to the existing item, the rest are appended.
pub fn merge_menu(into: &mut Vec<MenuItem>, items: Vec<MenuItem>) {
    for item in items {
        let existing = item
            .id
            .as_ref()
            .and_then(|id| into.iter_mut().find(|i| i.id.as_ref() == Some(id)));
        match existing {
            Some(target) => merge_menu(&mut target.children, item.children),
            None => into.push(item),
        }
    }
}

/// The right-click menus, merged across packages like the main menu.
#[derive(Debug, Clone, Default)]
pub struct ContextMenus {
    /// `Context.sublime-menu`: the text of a view.
    pub text: Vec<MenuItem>,
    /// `Side Bar.sublime-menu`: a file or folder of the side bar.
    pub side_bar: Vec<MenuItem>,
    /// `Side Bar Mount Point.sublime-menu`: added after the side bar's for a top folder.
    pub mount_point: Vec<MenuItem>,
    /// `Tab Context.sublime-menu`: a tab.
    pub tab: Vec<MenuItem>,
    /// `Tab Sidebar Context.sublime-menu`: an open file of the side bar (its tab's commands).
    pub tab_side_bar: Vec<MenuItem>,
    /// `Widget Context.sublime-menu`: a one-line field (the find, replace and where fields, the
    /// input panel).
    pub widget: Vec<MenuItem>,
    /// `Console Input Widget Context.sublime-menu`: the console's input field.
    pub console_input: Vec<MenuItem>,
    /// `Image Context.sublime-menu`: an image sheet.
    pub image: Vec<MenuItem>,
}

/// What a context menu was opened on: it picks the menu and fills its commands' arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuTarget {
    /// The text of a view, at a char (the first of the line for a click in its gutter), with the
    /// modifier keys held at the click (`ctrl`, `alt`, `shift`, `super`) for its `event`.
    Text {
        document: DocumentId,
        point: usize,
        modifier_keys: Vec<String>,
    },
    /// A file or folder of the side bar; `root` for a folder of the window (a mount point).
    SideBar {
        path: PathBuf,
        is_dir: bool,
        root: bool,
    },
    /// Several files and folders selected in the side bar (Ctrl / Shift + click), each with
    /// whether it is a folder: the side bar's menu, for all of them.
    SideBarSelection { entries: Vec<(PathBuf, bool)> },
    /// A tab: its group and its index among the group's tabs.
    Tab { group: usize, index: usize },
    /// An open file of the side bar: its tab, as [`Self::Tab`].
    SideBarTab { group: usize, index: usize },
    /// A one-line field, named by its widget id (`find_input`, `replace_input`…;
    /// [`CONSOLE_INPUT`] has its own menu).
    Widget { field: String },
    /// An image sheet: the picture's document. Its commands run with it active.
    Image { document: DocumentId },
}

/// The widget id of the console's input field.
pub const CONSOLE_INPUT: &str = "console_input";

impl ContextMenuTarget {
    /// The items of the menu this target opens.
    pub fn items(&self, menus: &ContextMenus) -> Vec<MenuItem> {
        match self {
            Self::Text { .. } => menus.text.clone(),
            Self::SideBar { root, .. } => {
                let mut items = menus.side_bar.clone();
                if *root {
                    items.extend(menus.mount_point.iter().cloned());
                }
                items
            }
            Self::SideBarSelection { .. } => menus.side_bar.clone(),
            Self::Tab { .. } => menus.tab.clone(),
            Self::SideBarTab { .. } => menus.tab_side_bar.clone(),
            Self::Widget { field } if field == CONSOLE_INPUT => menus.console_input.clone(),
            Self::Widget { .. } => menus.widget.clone(),
            Self::Image { .. } => menus.image.clone(),
        }
    }

    /// The command with the arguments Sublime fills in: in the side bar's menus `files`, `dirs`
    /// and `paths` (each only when the menu gives it, usually as `[]`), in a tab's `group` and
    /// `index` (given as `-1`). The text's menu fills nothing here (`event` depends on the command).
    pub fn fill(&self, command: &Command) -> Command {
        let Value::Object(args) = &command.args else {
            return command.clone();
        };
        let mut args = args.clone();
        match self {
            Self::Text { .. } | Self::Widget { .. } | Self::Image { .. } => {}
            Self::SideBar { path, is_dir, .. } => {
                fill_paths(&mut args, &[(path.clone(), *is_dir)]);
            }
            Self::SideBarSelection { entries } => fill_paths(&mut args, entries),
            Self::Tab { group, index } | Self::SideBarTab { group, index } => {
                for (key, value) in [("group", group), ("index", index)] {
                    if let Some(arg) = args.get_mut(key) {
                        *arg = Value::from(*value);
                    }
                }
            }
        }
        Command::with_args(&command.name, Value::Object(args))
    }
}

/// The side bar's `files` (the files of `entries`), `dirs` (its folders) and `paths` (all), each
/// only when the menu's command gives it.
fn fill_paths(args: &mut serde_json::Map<String, Value>, entries: &[(PathBuf, bool)]) {
    for (key, wanted) in [
        ("files", Some(false)),
        ("dirs", Some(true)),
        ("paths", None),
    ] {
        if let Some(value) = args.get_mut(key) {
            *value = Value::Array(
                entries
                    .iter()
                    .filter(|(_, is_dir)| wanted.is_none_or(|dir| dir == *is_dir))
                    .map(|(path, _)| Value::String(path.display().to_string()))
                    .collect(),
            );
        }
    }
}

/// A menu with every command's arguments filled for `target`.
pub fn fill_menu(items: &[MenuItem], target: &ContextMenuTarget) -> Vec<MenuItem> {
    items
        .iter()
        .map(|item| MenuItem {
            command: item.command.as_ref().map(|c| target.fill(c)),
            children: fill_menu(&item.children, target),
            ..item.clone()
        })
        .collect()
}

/// An entry of the command palette (`.sublime-commands`).
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub caption: String,
    pub command: Command,
}
