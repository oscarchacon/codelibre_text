//! Tests for the presenters (application state → view models).

use std::path::PathBuf;

use entities::command::Command;
use entities::find::FindOptions;
use entities::keymap::{Binding, KeyCombo};
use entities::menu::MenuItem;
use entities::selection::Selection;
use interface_adapters::presenters::{dialogs, editor as editor_presenter, find_panel, menu, quick_panel, side_bar, status_bar, tabs, window};
use interface_adapters::view_models::{DialogViewModel, TreeRowKind};
use interface_adapters::view_state::{QuickPanel, ViewState};
use serde_json::json;
use test_support::{open, World};
use use_cases::editing::TextCommand;
use use_cases::search::SearchKind;
use use_cases::Editor;

fn editor_with_text(world: &World, text: &str, selection: Selection) -> Editor {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, selection);
    editor
}

// ----- window title and tabs -----------------------------------------------------------------------

#[test]
fn the_window_title_shows_the_path_and_unsaved_changes() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    assert_eq!(window::present_title(&editor), "untitled - CodeLibre Text");
    open(&mut editor, "/p/a.txt");
    let path = PathBuf::from("/p/a.txt").display().to_string();
    assert_eq!(window::present_title(&editor), format!("{path} - CodeLibre Text"));
    editor.run_text_command(TextCommand::Insert("x".into()));
    assert_eq!(window::present_title(&editor), format!("{path} • - CodeLibre Text"));
    editor.close_all_documents();
    editor.resolve_pending_close(use_cases::editor::CloseChoice::DontSave);
    assert_eq!(window::present_title(&editor), "CodeLibre Text");
}

#[test]
fn tabs_show_names_dirty_state_and_the_active_tab() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.new_file();
    editor.run_text_command(TextCommand::Insert("notes".into()));
    let tabs = tabs::present(&editor);
    assert_eq!(tabs.len(), 2);
    assert_eq!((tabs[0].title.as_str(), tabs[0].dirty, tabs[0].active), ("a.txt", false, false));
    assert_eq!(tabs[0].tooltip, PathBuf::from("/p/a.txt").display().to_string());
    assert_eq!((tabs[1].title.as_str(), tabs[1].dirty, tabs[1].active), ("notes", true, true));
    assert_eq!(tabs[1].tooltip, "notes");
}

// ----- side bar ------------------------------------------------------------------------------------

#[test]
fn the_side_bar_lists_open_files() {
    let world = World::new();
    let editor = editor_with_text(&world, "draft", Selection::caret(0));
    let side = side_bar::present(&editor, &mut ViewState::default());
    assert_eq!(side.open_files.len(), 1);
    assert_eq!(side.open_files[0].name, "draft");
    assert!(side.open_files[0].dirty && side.open_files[0].active);
    assert!(side.folders.is_empty());
}

#[test]
fn collapsed_folders_show_only_their_root() {
    let world = World::new().with_dir("/proj").with_file("/proj/a.txt", "");
    let editor = world.editor_opening(&["/proj"]);
    let side = side_bar::present(&editor, &mut ViewState::default());
    assert_eq!(side.folders.len(), 1);
    assert_eq!(side.folders[0].name, "proj");
    assert!(matches!(side.folders[0].kind, TreeRowKind::Folder { expanded: false, is_root: true }));
}

#[test]
fn expanded_folders_show_their_contents_indented() {
    let world = World::new()
        .with_dir("/proj")
        .with_dir("/proj/src")
        .with_dir("/proj/.git")
        .with_file("/proj/src/main.rs", "")
        .with_file("/proj/README.md", "");
    let mut editor = world.editor_opening(&["/proj"]);
    open(&mut editor, "/proj/src/main.rs");
    let mut view = ViewState::default();
    view.expanded_folders.insert(PathBuf::from("/proj"));
    view.expanded_folders.insert(PathBuf::from("/proj/src"));

    let side = side_bar::present(&editor, &mut view);
    let rows: Vec<(usize, &str, bool)> = side.folders.iter().map(|r| (r.depth, r.name.as_str(), r.active)).collect();
    assert_eq!(
        rows,
        vec![(0, "proj", false), (1, "src", false), (2, "main.rs", true), (1, "README.md", false)],
        ".git is excluded, folders come first, the open file is marked"
    );
    assert!(matches!(side.folders[1].kind, TreeRowKind::Folder { expanded: true, is_root: false }));
    assert!(matches!(side.folders[2].kind, TreeRowKind::File));
}

#[test]
fn folder_contents_are_cached_until_refreshed() {
    let world = World::new().with_dir("/proj").with_file("/proj/a.txt", "");
    let editor = world.editor_opening(&["/proj"]);
    let mut view = ViewState::default();
    view.expanded_folders.insert(PathBuf::from("/proj"));
    assert_eq!(side_bar::present(&editor, &mut view).folders.len(), 2);

    world.disk.put("/proj/b.txt", "");
    assert_eq!(side_bar::present(&editor, &mut view).folders.len(), 2, "cached");
    view.forget_folder_listings();
    assert_eq!(side_bar::present(&editor, &mut view).folders.len(), 3);
}

// ----- status bar ----------------------------------------------------------------------------------

#[test]
fn the_status_bar_shows_the_caret_and_document_details() {
    let world = World::new();
    let editor = editor_with_text(&world, "ab\ncdef", Selection::caret(5));
    let status = status_bar::present(&editor);
    assert_eq!(status.position, "Line 2, Column 3");
    assert_eq!(status.indentation, "Tab Size: 4");
    assert_eq!(status.line_ending, "Unix");
    assert_eq!(status.encoding, "UTF-8");
    assert_eq!(status.syntax, "Plain Text");
    assert_eq!(status.message, None);
}

#[test]
fn the_status_bar_counts_selected_characters() {
    let world = World::new();
    let editor = editor_with_text(&world, "abc", Selection::new(0, 3));
    assert_eq!(status_bar::present(&editor).position, "Line 1, Column 4; 3 characters selected");
    let editor = editor_with_text(&world, "abc", Selection::new(0, 1));
    assert_eq!(status_bar::present(&editor).position, "Line 1, Column 2; 1 character selected");
}

#[test]
fn the_status_bar_shows_spaces_and_messages() {
    let world = World::new().with_preferences(json!({ "translate_tabs_to_spaces": true, "tab_size": 2 }));
    let mut editor = world.editor();
    editor.notify("Saved");
    let status = status_bar::present(&editor);
    assert_eq!(status.indentation, "Spaces: 2");
    assert_eq!(status.message.as_deref(), Some("Saved"));
}

#[test]
fn the_status_bar_is_empty_without_documents() {
    let world = World::new();
    let mut editor = world.editor();
    editor.close_all_documents();
    let status = status_bar::present(&editor);
    assert!(status.position.is_empty() && status.syntax.is_empty());
}

// ----- menu ----------------------------------------------------------------------------------------

#[test]
fn the_menu_shows_shortcuts_and_check_marks() {
    let item = |caption: &str, command: Option<Command>, checkbox: bool, children: Vec<MenuItem>| MenuItem {
        caption: caption.into(),
        id: None,
        command,
        checkbox,
        children,
    };
    let world = World::new()
        .with_preferences(json!({ "word_wrap": true }))
        .with_binding(Binding { keys: vec![KeyCombo::parse("ctrl+s").unwrap()], command: Command::new("save"), context: vec![] })
        .with_menu(item(
            "File",
            None,
            false,
            vec![
                item("Save", Some(Command::new("save")), false, vec![]),
                item("-", None, false, vec![]),
                item("Word Wrap", Some(Command::with_args("toggle_setting", json!({ "setting": "word_wrap" }))), true, vec![]),
                item("Line Numbers", Some(Command::with_args("toggle_setting", json!({ "setting": "line_numbers" }))), true, vec![]),
            ],
        ))
        .with_menu(item("Empty", None, false, vec![]));
    let editor = world.editor();
    let menu = menu::present(&editor);
    assert_eq!(menu.len(), 1, "top-level menus without items are hidden");
    let file = &menu[0].children;
    assert_eq!(file[0].shortcut.as_deref(), Some("Ctrl+S"));
    assert_eq!(file[0].checked, None);
    assert!(file[1].separator);
    assert_eq!(file[2].checked, Some(true));
    assert_eq!(file[3].checked, Some(false));
}

// ----- quick panel ---------------------------------------------------------------------------------

#[test]
fn the_quick_panel_searches_once_per_query_and_takes_focus_once() {
    let world = World::new().with_file("/p/a.txt", "a\nb");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let mut view = ViewState::default();
    assert!(quick_panel::present(&editor, &mut view).is_none());

    view.quick_panel = Some(QuickPanel::new(SearchKind::Goto, ":2".into()));
    let first = quick_panel::present(&editor, &mut view).unwrap();
    assert_eq!(first.placeholder, "Goto Anything — file name, :line, @symbol");
    assert_eq!(first.items[0].caption, "Go to line 2");
    assert!(first.take_focus);
    assert!(!quick_panel::present(&editor, &mut view).unwrap().take_focus);
    assert_eq!(view.quick_panel.as_ref().unwrap().results_query.as_deref(), Some(":2"));
}

#[test]
fn the_quick_panel_keeps_the_selection_within_the_results() {
    let world = World::new();
    let editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::ColorSchemes, "mono".into());
    panel.selected = 5;
    view.quick_panel = Some(panel);
    let model = quick_panel::present(&editor, &mut view).unwrap();
    assert_eq!(model.placeholder, "Select Color Scheme");
    assert_eq!(model.items.len(), 1);
    assert_eq!(model.selected, 0);
}

#[test]
fn the_command_palette_placeholder() {
    let world = World::new();
    let editor = world.editor();
    let mut view = ViewState::default();
    view.quick_panel = Some(QuickPanel::new(SearchKind::Commands, String::new()));
    assert_eq!(quick_panel::present(&editor, &mut view).unwrap().placeholder, "Command Palette");
}

// ----- find panel ----------------------------------------------------------------------------------

#[test]
fn the_find_panel_is_hidden_until_opened() {
    let world = World::new();
    let mut editor = world.editor();
    assert!(find_panel::present(&mut editor, &mut ViewState::default()).is_none());
}

#[test]
fn the_find_panel_shows_options_and_the_match_count() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "ab ab", Selection::caret(0));
    editor.set_find_options(FindOptions { query: "ab".into(), whole_word: true, ..Default::default() });
    let mut view = ViewState::default();
    view.find_panel.visible = true;
    view.find_panel.replace_mode = true;
    view.find_panel.focus_requested = true;

    let panel = find_panel::present(&mut editor, &mut view).unwrap();
    assert_eq!(panel.query, "ab");
    assert!(panel.whole_word && panel.wrap && panel.replace_mode && !panel.regex);
    assert_eq!(panel.status, "2 matches");
    assert!(!panel.has_error);
    assert!(panel.focus_requested);
    assert!(!find_panel::present(&mut editor, &mut view).unwrap().focus_requested, "focus is taken once");
}

#[test]
fn the_find_panel_shows_regex_errors_and_nothing_for_an_empty_query() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "abc", Selection::caret(0));
    let mut view = ViewState::default();
    view.find_panel.visible = true;
    assert_eq!(find_panel::present(&mut editor, &mut view).unwrap().status, "");

    editor.set_find_options(FindOptions { query: "(".into(), regex: true, ..Default::default() });
    let panel = find_panel::present(&mut editor, &mut view).unwrap();
    assert!(panel.has_error);
    assert!(panel.status.starts_with("Error: "));
}

// ----- editor --------------------------------------------------------------------------------------

#[test]
fn the_editor_view_model_reflects_the_document_and_its_settings() {
    let world = World::new()
        .with_file("/p/a.py", "x")
        .with_syntax_settings("Python", json!({ "tab_size": 2, "word_wrap": true, "rulers": [79] }));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    let model = editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap();
    let doc = editor.active_document().unwrap();
    assert_eq!(model.document, doc.id);
    assert_eq!(model.selection_revision, doc.selection_revision());
    assert_eq!(model.syntax, "Python");
    assert_eq!(model.color_scheme, "Monokai");
    assert_eq!(model.font_size, 11.0);
    assert_eq!(model.tab_size, 2);
    assert!(model.word_wrap && model.line_numbers && model.highlight_line && model.auto_indent);
    assert_eq!(model.rulers, vec![79]);
    assert!(!model.read_only);
}

#[test]
fn word_wrap_must_be_exactly_true() {
    let world = World::new().with_preferences(json!({ "word_wrap": "auto" }));
    let mut editor = world.editor();
    assert!(!editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap().word_wrap);
}

#[test]
fn font_sizes_are_clamped() {
    let world = World::new().with_preferences(json!({ "font_size": 200 }));
    let mut editor = world.editor();
    assert_eq!(editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap().font_size, 72.0);
}

#[test]
fn the_text_takes_focus_only_when_nothing_else_needs_it() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(editor_presenter::present(&mut editor, &mut view).unwrap().take_focus);
    assert!(!editor_presenter::present(&mut editor, &mut view).unwrap().take_focus, "taken once");

    view.focus_editor = true;
    view.quick_panel = Some(QuickPanel::new(SearchKind::Goto, String::new()));
    assert!(!editor_presenter::present(&mut editor, &mut view).unwrap().take_focus);
}

#[test]
fn find_matches_are_only_highlighted_while_the_find_panel_is_open() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "ab ab", Selection::caret(0));
    editor.set_find_options(FindOptions { query: "ab".into(), ..Default::default() });
    let mut view = ViewState::default();
    assert!(editor_presenter::present(&mut editor, &mut view).unwrap().find_matches.is_empty());
    view.find_panel.visible = true;
    assert_eq!(editor_presenter::present(&mut editor, &mut view).unwrap().find_matches, vec![0..2, 3..5]);
}

#[test]
fn there_is_no_editor_without_documents() {
    let world = World::new();
    let mut editor = world.editor();
    editor.close_all_documents();
    assert!(editor_presenter::present(&mut editor, &mut ViewState::default()).is_none());
}

// ----- dialogs -------------------------------------------------------------------------------------

#[test]
fn the_close_dialog_names_the_document() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "draft", Selection::caret(0));
    assert!(dialogs::present_close_dialog(&editor).is_none());
    editor.close_active_document();
    match dialogs::present_close_dialog(&editor) {
        Some(DialogViewModel::SaveChanges { document_name }) => assert_eq!(document_name, "draft"),
        _ => panic!("expected the save changes dialog"),
    }
}

#[test]
fn the_exit_dialog() {
    let world = World::new().with_preferences(json!({ "hot_exit": false }));
    let mut editor = editor_with_text(&world, "draft", Selection::caret(0));
    editor.request_exit(Vec::new());
    assert!(matches!(dialogs::present_close_dialog(&editor), Some(DialogViewModel::ExitWithUnsavedChanges)));
}

#[test]
fn about_and_load_errors_windows() {
    let world = World::new();
    world.resources.errors.borrow_mut().push("broken file".into());
    let editor = world.editor();
    let mut view = ViewState::default();
    assert!(dialogs::present_about(&editor, &view).is_none());
    assert!(dialogs::present_load_errors(&editor, &view).is_none());

    view.show_about = true;
    view.show_load_errors = true;
    let about = dialogs::present_about(&editor, &view).unwrap();
    assert_eq!(about.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(about.packages, PathBuf::from("/packages").display().to_string());
    assert!(about.sublime_packages.is_none());
    assert_eq!(dialogs::present_load_errors(&editor, &view).unwrap(), vec!["broken file"]);
}
