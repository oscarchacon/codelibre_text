//! Tests for `view.export_to_html` (`use_cases::html_export`). The fake syntax gateway gives every
//! line the syntax's base scope (white), and text between double quotes `string.quoted.double`
//! (red and bold).

use std::path::Path;

use entities::selection::Selection;
use test_support::World;
use use_cases::html_export::HtmlExportOptions;
use use_cases::Editor;

fn python(world: &World) -> Editor {
    world.disk.put("/p/a.py", "x = \"hi\"\n");
    let mut editor = world.editor();
    editor.open_file(Path::new("/p/a.py"));
    editor
}

fn id(editor: &Editor) -> u64 {
    editor.active_document().unwrap().id
}

fn enclosing() -> HtmlExportOptions {
    HtmlExportOptions { minihtml: false, enclosing_tags: true, font_size: true, font_family: true }
}

#[test]
fn the_whole_view_gets_a_span_per_scope() {
    let world = World::new();
    let editor = python(&world);
    let html = editor.export_to_html(id(&editor), None, enclosing()).unwrap();
    assert_eq!(
        html,
        concat!(
            "<div style=\"color: #ffffff; background-color: #000000; font-family: monospace; font-size: 11pt;\">",
            "<pre style=\"margin: 0;\">",
            "<span style=\"color: #ffffff;\">x = </span>",
            "<span style=\"color: #ff0000; font-weight: bold;\">\"hi\"</span>",
            "<span style=\"color: #ffffff;\">\n</span>",
            "</pre></div>"
        )
    );
}

#[test]
fn the_regions_asked_for_are_the_ones_exported() {
    let world = World::new();
    let editor = python(&world);
    // Without the enclosing tags there is no background and no font styling.
    let html = editor
        .export_to_html(id(&editor), Some(&[Selection::new(4, 8)]), HtmlExportOptions { font_size: true, ..Default::default() })
        .unwrap();
    assert_eq!(
        html,
        "<pre style=\"margin: 0;\"><span style=\"color: #ff0000; font-weight: bold;\">\"hi\"</span></pre>"
    );
}

#[test]
fn the_font_size_and_family_are_left_out_when_not_asked_for() {
    let world = World::new();
    let editor = python(&world);
    let html = editor
        .export_to_html(id(&editor), None, HtmlExportOptions { enclosing_tags: true, ..Default::default() })
        .unwrap();
    assert!(html.starts_with("<div style=\"color: #ffffff; background-color: #000000;\">"), "{html}");
}

#[test]
fn the_text_is_escaped() {
    let world = World::new();
    world.disk.put("/p/a.py", "a < b & c\n");
    let mut editor = world.editor();
    editor.open_file(Path::new("/p/a.py"));
    let html = editor.export_to_html(id(&editor), None, HtmlExportOptions::default()).unwrap();
    assert_eq!(html, "<pre style=\"margin: 0;\"><span style=\"color: #ffffff;\">a &lt; b &amp; c\n</span></pre>");
}

#[test]
fn several_regions_follow_one_another() {
    let world = World::new();
    let editor = python(&world);
    let html = editor
        .export_to_html(id(&editor), Some(&[Selection::new(0, 1), Selection::new(4, 8)]), HtmlExportOptions::default())
        .unwrap();
    assert_eq!(
        html,
        concat!(
            "<pre style=\"margin: 0;\">",
            "<span style=\"color: #ffffff;\">x</span>",
            "\n",
            "<span style=\"color: #ff0000; font-weight: bold;\">\"hi\"</span>",
            "</pre>"
        )
    );
}
