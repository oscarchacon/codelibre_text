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
    /// `Tab Context.sublime-menu`: a tab (also an open file of the side bar).
    pub tab: Vec<MenuItem>,
}

/// What a context menu was opened on: it picks the menu and fills its commands' arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuTarget {
    /// The text of a view, at a char.
    Text { document: DocumentId, point: usize },
    /// A file or folder of the side bar; `root` for a folder of the window (a mount point).
    SideBar { path: PathBuf, is_dir: bool, root: bool },
    /// A tab: its group and its index among the group's tabs.
    Tab { group: usize, index: usize },
}

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
            Self::Tab { .. } => menus.tab.clone(),
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
            Self::Text { .. } => {}
            Self::SideBar { path, is_dir, .. } => {
                let path = Value::String(path.display().to_string());
                for (key, wanted) in [("files", !is_dir), ("dirs", *is_dir), ("paths", true)] {
                    if let Some(value) = args.get_mut(key) {
                        *value = Value::Array(if wanted { vec![path.clone()] } else { Vec::new() });
                    }
                }
            }
            Self::Tab { group, index } => {
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
