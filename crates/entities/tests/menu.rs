//! Tests for `entities::menu`.

use entities::menu::{merge_menu, MenuItem};

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
fn a_dash_caption_is_a_separator() {
    assert!(item("-", None, vec![]).is_separator());
    assert!(!item("File", None, vec![]).is_separator());
}

#[test]
fn packages_add_children_to_items_with_the_same_id() {
    let mut menu = vec![item("Tools", Some("tools"), vec![item("A", None, vec![])])];
    merge_menu(
        &mut menu,
        vec![
            item("", Some("tools"), vec![item("B", None, vec![])]),
            item("Extra", None, vec![]),
        ],
    );
    assert_eq!(menu.len(), 2);
    let tools: Vec<&str> = menu[0]
        .children
        .iter()
        .map(|c| c.caption.as_str())
        .collect();
    assert_eq!(tools, vec!["A", "B"]);
    assert_eq!(menu[1].caption, "Extra");
}

#[test]
fn merging_is_recursive() {
    let mut menu = vec![item(
        "Preferences",
        Some("preferences"),
        vec![item("Package Settings", Some("package-settings"), vec![])],
    )];
    let package_menu = vec![item(
        "",
        Some("preferences"),
        vec![item(
            "",
            Some("package-settings"),
            vec![item("My Package", None, vec![])],
        )],
    )];
    merge_menu(&mut menu, package_menu);
    assert_eq!(menu[0].children.len(), 1);
    assert_eq!(menu[0].children[0].children[0].caption, "My Package");
}

// ----- context menus -------------------------------------------------------------------------------

mod context_menus {
    use std::path::PathBuf;

    use entities::command::Command;
    use entities::menu::{fill_menu, ContextMenuTarget, ContextMenus, MenuItem, CONSOLE_INPUT};
    use serde_json::json;

    fn entry(command: &str, args: serde_json::Value) -> MenuItem {
        MenuItem {
            caption: command.into(),
            id: None,
            command: Some(Command::with_args(command, args)),
            checkbox: false,
            children: vec![],
        }
    }

    #[test]
    fn the_side_bar_fills_the_files_dirs_and_paths_the_menu_gives() {
        let file = ContextMenuTarget::SideBar {
            path: PathBuf::from("/p/a.txt"),
            is_dir: false,
            root: false,
        };
        let filled = file.fill(&Command::with_args(
            "x",
            json!({ "files": [], "dirs": [], "paths": [], "other": 1 }),
        ));
        assert_eq!(
            filled.args,
            json!({ "files": ["/p/a.txt"], "dirs": [], "paths": ["/p/a.txt"], "other": 1 })
        );

        let folder = ContextMenuTarget::SideBar {
            path: PathBuf::from("/p"),
            is_dir: true,
            root: true,
        };
        assert_eq!(
            folder
                .fill(&Command::with_args("x", json!({ "dirs": [] })))
                .args,
            json!({ "dirs": ["/p"] })
        );
        assert_eq!(
            folder.fill(&Command::new("y")).args,
            serde_json::Value::Null,
            "no arguments, nothing filled"
        );
    }

    #[test]
    fn a_tab_fills_its_group_and_index() {
        let tab = ContextMenuTarget::Tab { group: 1, index: 2 };
        let filled = tab.fill(&Command::with_args(
            "close_by_index",
            json!({ "group": -1, "index": -1 }),
        ));
        assert_eq!(filled.args, json!({ "group": 1, "index": 2 }));
    }

    #[test]
    fn a_top_folder_adds_the_mount_point_menu() {
        let menus = ContextMenus {
            side_bar: vec![entry("a", json!({}))],
            mount_point: vec![entry("remove_folder", json!({ "dirs": [] }))],
            ..ContextMenus::default()
        };
        let inner = ContextMenuTarget::SideBar {
            path: PathBuf::from("/p/sub"),
            is_dir: true,
            root: false,
        };
        assert_eq!(inner.items(&menus).len(), 1);
        let root = ContextMenuTarget::SideBar {
            path: PathBuf::from("/p"),
            is_dir: true,
            root: true,
        };
        let filled = fill_menu(&root.items(&menus), &root);
        assert_eq!(filled.len(), 2);
        assert_eq!(
            filled[1].command.as_ref().unwrap().args,
            json!({ "dirs": ["/p"] })
        );
    }

    #[test]
    fn submenus_are_filled_too() {
        let mut parent = entry("parent", json!({}));
        parent.command = None;
        parent.children = vec![entry("child", json!({ "paths": [] }))];
        let target = ContextMenuTarget::SideBar {
            path: PathBuf::from("/p/a"),
            is_dir: false,
            root: false,
        };
        let filled = fill_menu(&[parent], &target);
        assert_eq!(
            filled[0].children[0].command.as_ref().unwrap().args,
            json!({ "paths": ["/p/a"] })
        );
    }

    #[test]
    fn a_selection_in_the_side_bar_fills_all_its_files_and_folders() {
        let target = ContextMenuTarget::SideBarSelection {
            entries: vec![
                (PathBuf::from("/p/a.txt"), false),
                (PathBuf::from("/p/sub"), true),
                (PathBuf::from("/p/b.txt"), false),
            ],
        };
        let filled = target.fill(&Command::with_args(
            "x",
            json!({ "files": [], "dirs": [], "paths": [] }),
        ));
        assert_eq!(
            filled.args,
            json!({
                "files": ["/p/a.txt", "/p/b.txt"],
                "dirs": ["/p/sub"],
                "paths": ["/p/a.txt", "/p/sub", "/p/b.txt"],
            })
        );
        let menus = ContextMenus {
            side_bar: vec![entry("a", json!({}))],
            mount_point: vec![entry("remove_folder", json!({ "dirs": [] }))],
            ..ContextMenus::default()
        };
        assert_eq!(target.items(&menus).len(), 1, "no mount point entries");
    }

    #[test]
    fn open_files_images_and_the_console_s_field_have_their_own_menus() {
        let menus = ContextMenus {
            tab: vec![entry("tab", json!({}))],
            tab_side_bar: vec![entry("close_by_index", json!({ "group": -1, "index": -1 }))],
            widget: vec![entry("widget", json!({}))],
            console_input: vec![entry("console", json!({}))],
            image: vec![entry("copy_path", json!({}))],
            ..ContextMenus::default()
        };
        let caption = |target: &ContextMenuTarget| target.items(&menus)[0].caption.clone();
        let open_file = ContextMenuTarget::SideBarTab { group: 0, index: 3 };
        assert_eq!(caption(&open_file), "close_by_index");
        assert_eq!(
            fill_menu(&open_file.items(&menus), &open_file)[0]
                .command
                .as_ref()
                .unwrap()
                .args,
            json!({ "group": 0, "index": 3 })
        );
        let console = ContextMenuTarget::Widget {
            field: CONSOLE_INPUT.into(),
        };
        assert_eq!(caption(&console), "console");
        let find = ContextMenuTarget::Widget {
            field: "find_input".into(),
        };
        assert_eq!(caption(&find), "widget");
        assert_eq!(
            caption(&ContextMenuTarget::Image { document: 4 }),
            "copy_path"
        );
    }
}
