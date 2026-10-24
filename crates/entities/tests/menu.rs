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
