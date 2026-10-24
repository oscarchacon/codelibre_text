//! The main menu and command palette entries.

use crate::command::Command;

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

/// An entry of the command palette (`.sublime-commands`).
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub caption: String,
    pub command: Command,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(caption: &str, id: Option<&str>, children: Vec<MenuItem>) -> MenuItem {
        MenuItem {
            caption: caption.into(),
            id: id.map(String::from),
            command: None,
            checkbox: false,
            children,
        }
    }

    #[test]
    fn merges_children_by_id() {
        let mut menu = vec![item("Tools", Some("tools"), vec![item("A", None, vec![])])];
        merge_menu(&mut menu, vec![item("", Some("tools"), vec![item("B", None, vec![])]), item("Extra", None, vec![])]);
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].children.len(), 2);
    }
}
