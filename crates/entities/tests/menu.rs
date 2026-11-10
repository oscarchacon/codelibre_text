//! Tests for `entities::menu`.

use entities::menu::{merge_menu, MenuItem};

fn item(caption: &str, id: Option<&str>, children: Vec<MenuItem>) -> MenuItem {
    MenuItem { caption: caption.into(), id: id.map(String::from), command: None, checkbox: false, children }
}

#[test]
fn a_dash_caption_is_a_separator() {
    assert!(item("-", None, vec![]).is_separator());
    assert!(!item("File", None, vec![]).is_separator());
}

#[test]
fn packages_add_children_to_items_with_the_same_id() {
    let mut menu = vec![item("Tools", Some("tools"), vec![item("A", None, vec![])])];
    merge_menu(&mut menu, vec![item("", Some("tools"), vec![item("B", None, vec![])]), item("Extra", None, vec![])]);
    assert_eq!(menu.len(), 2);
    let tools: Vec<&str> = menu[0].children.iter().map(|c| c.caption.as_str()).collect();
    assert_eq!(tools, vec!["A", "B"]);
    assert_eq!(menu[1].caption, "Extra");
}

#[test]
fn merging_is_recursive() {
    let mut menu = vec![item("Preferences", Some("preferences"), vec![item("Package Settings", Some("package-settings"), vec![])])];
    let package_menu = vec![item(
        "",
        Some("preferences"),
        vec![item("", Some("package-settings"), vec![item("My Package", None, vec![])])],
    )];
    merge_menu(&mut menu, package_menu);
    assert_eq!(menu[0].children.len(), 1);
    assert_eq!(menu[0].children[0].children[0].caption, "My Package");
}

// ----- context menus -------------------------------------------------------------------------------

mod context_menus {
    use std::path::PathBuf;

    use entities::command::Command;
    use entities::menu::{fill_menu, ContextMenuTarget, ContextMenus, MenuItem};
    use serde_json::json;

    fn entry(command: &str, args: serde_json::Value) -> MenuItem {
        MenuItem { caption: command.into(), id: None, command: Some(Command::with_args(command, args)), checkbox: false, children: vec![] }
    }

    #[test]
    fn the_side_bar_fills_the_files_dirs_and_paths_the_menu_gives() {
        let file = ContextMenuTarget::SideBar { path: PathBuf::from("/p/a.txt"), is_dir: false, root: false };
        let filled = file.fill(&Command::with_args("x", json!({ "files": [], "dirs": [], "paths": [], "other": 1 })));
        assert_eq!(filled.args, json!({ "files": ["/p/a.txt"], "dirs": [], "paths": ["/p/a.txt"], "other": 1 }));

        let folder = ContextMenuTarget::SideBar { path: PathBuf::from("/p"), is_dir: true, root: true };
        assert_eq!(folder.fill(&Command::with_args("x", json!({ "dirs": [] }))).args, json!({ "dirs": ["/p"] }));
        assert_eq!(folder.fill(&Command::new("y")).args, serde_json::Value::Null, "no arguments, nothing filled");
    }

    #[test]
    fn a_tab_fills_its_group_and_index() {
        let tab = ContextMenuTarget::Tab { group: 1, index: 2 };
        let filled = tab.fill(&Command::with_args("close_by_index", json!({ "group": -1, "index": -1 })));
        assert_eq!(filled.args, json!({ "group": 1, "index": 2 }));
    }

    #[test]
    fn a_top_folder_adds_the_mount_point_menu() {
        let menus = ContextMenus {
            side_bar: vec![entry("a", json!({}))],
            mount_point: vec![entry("remove_folder", json!({ "dirs": [] }))],
            ..ContextMenus::default()
        };
        let inner = ContextMenuTarget::SideBar { path: PathBuf::from("/p/sub"), is_dir: true, root: false };
        assert_eq!(inner.items(&menus).len(), 1);
        let root = ContextMenuTarget::SideBar { path: PathBuf::from("/p"), is_dir: true, root: true };
        let filled = fill_menu(&root.items(&menus), &root);
        assert_eq!(filled.len(), 2);
        assert_eq!(filled[1].command.as_ref().unwrap().args, json!({ "dirs": ["/p"] }));
    }

    #[test]
    fn submenus_are_filled_too() {
        let mut parent = entry("parent", json!({}));
        parent.command = None;
        parent.children = vec![entry("child", json!({ "paths": [] }))];
        let target = ContextMenuTarget::SideBar { path: PathBuf::from("/p/a"), is_dir: false, root: false };
        let filled = fill_menu(&[parent], &target);
        assert_eq!(filled[0].children[0].command.as_ref().unwrap().args, json!({ "paths": ["/p/a"] }));
    }
}
