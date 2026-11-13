//! Tests for the presenters (application state → view models).

use std::path::PathBuf;

use entities::command::Command;
use entities::find::FindOptions;
use entities::keymap::{Binding, KeyCombo};
use entities::menu::MenuItem;
use entities::selection::Selection;
use interface_adapters::presenters::{
    dialogs, editor as editor_presenter, find_panel, menu, output_panel, quick_panel, side_bar, status_bar, tabs, window,
};
use interface_adapters::view_models::{DialogViewModel, MinimapViewModel, TreeRowKind};
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
fn the_side_bar_reads_its_folders_again_after_a_side_bar_command() {
    let world = World::new().with_dir("/proj").with_file("/proj/a.txt", "").with_file("/proj/b.txt", "");
    let mut editor = world.editor_opening(&["/proj"]);
    let mut view = ViewState::default();
    view.expanded_folders.insert(PathBuf::from("/proj"));
    assert_eq!(side_bar::present(&editor, &mut view).folders.len(), 3);
    editor.delete_file(&Command::with_args("delete_file", json!({ "files": ["/proj/a.txt"], "prompt": false })));
    let names: Vec<String> = side_bar::present(&editor, &mut view).folders.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, ["proj", "b.txt"]);
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
    assert_eq!(file[0].shortcut, Some(KeyCombo::parse("ctrl+s").unwrap().display()));
    assert_eq!(file[0].checked, None);
    assert!(file[1].separator);
    assert_eq!(file[2].checked, Some(true));
    assert_eq!(file[3].checked, Some(false));
}

#[test]
fn the_build_system_menu_lists_the_build_systems_and_their_variants_and_checks_the_one_set() {
    let item = |caption: &str, command: Option<Command>, checkbox: bool, children: Vec<MenuItem>| MenuItem {
        caption: caption.into(),
        id: None,
        command,
        checkbox,
        children,
    };
    let automatic = Command::with_args("set_build_system", json!({ "file": "" }));
    let world = World::new()
        .with_resource("Packages/U/Zig.sublime-build", r#"{ "cmd": ["zig"] }"#)
        .with_resource("Packages/U/Make.sublime-build", r#"{ "cmd": ["make"], "variants": [{ "name": "Clean", "cmd": ["make", "clean"] }] }"#)
        .with_menu(item(
            "Tools",
            None,
            false,
            vec![item("Build System", None, false, vec![item("Automatic", Some(automatic), true, vec![]), item("-", None, false, vec![]), item("", Some(Command::new("$build_systems")), false, vec![])])],
        ));
    let mut editor = world.editor();
    editor.load_packages();
    let systems = |editor: &Editor| menu::present(editor)[0].children[0].children.iter().map(|e| (e.caption.clone(), e.checked)).collect::<Vec<_>>();
    assert_eq!(
        systems(&editor),
        [
            ("Automatic".into(), Some(true)),
            ("-".into(), None),
            ("Make".into(), Some(false)),
            ("Make - Clean".into(), Some(false)),
            ("Zig".into(), Some(false))
        ]
    );
    editor.set_build_system("Packages/U/Zig.sublime-build", "");
    assert_eq!(systems(&editor)[0].1, Some(false));
    assert_eq!(systems(&editor)[4], ("Zig".into(), Some(true)));
    let entry = &menu::present(&editor)[0].children[0].children[4];
    assert_eq!(entry.command.as_ref().unwrap().str_arg("file"), "Packages/U/Zig.sublime-build");

    editor.set_build_system("Packages/U/Make.sublime-build", "Clean");
    let checked: Vec<_> = systems(&editor).into_iter().filter(|(_, checked)| *checked == Some(true)).collect();
    assert_eq!(checked, [("Make - Clean".into(), Some(true))], "only the variant, not its build");
    let entry = &menu::present(&editor)[0].children[0].children[3];
    assert_eq!(entry.command.as_ref().unwrap().str_arg("variant"), "Clean");
}

#[test]
fn package_settings_show_once_a_package_adds_some() {
    let item = |caption: &str, command: Option<Command>, checkbox: bool, children: Vec<MenuItem>| MenuItem {
        caption: caption.into(),
        id: None,
        command,
        checkbox,
        children,
    };
    let preferences = |children| item("Preferences", None, false, vec![item("Settings", Some(Command::new("edit_settings")), false, vec![]), item("-", None, false, vec![]), item("Package Settings", None, false, children)]);
    let world = World::new().with_menu(preferences(vec![]));
    let menu = menu::present(&World::editor(&world));
    let captions: Vec<&str> = menu[0].children.iter().map(|e| e.caption.as_str()).collect();
    assert_eq!(captions, ["Settings"], "no empty submenu, no separator left at the end");

    let linter = item("Linter", None, false, vec![item("Settings", Some(Command::new("edit_settings")), false, vec![])]);
    let world = World::new().with_menu(preferences(vec![linter]));
    let menu = menu::present(&World::editor(&world));
    let entries = &menu[0].children;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[2].caption, "Package Settings");
    assert_eq!(entries[2].children[0].caption, "Linter");
}

#[test]
fn on_macos_about_preferences_and_quit_move_to_the_application_menu() {
    let item = |caption: &str, id: Option<&str>, command: Option<&str>, children: Vec<MenuItem>| MenuItem {
        caption: caption.into(),
        id: id.map(Into::into),
        command: command.map(Command::new),
        checkbox: false,
        children,
    };
    let separator = || item("-", None, None, vec![]);
    let world = World::new()
        .with_menu(item("File", Some("file"), None, vec![item("Save", None, Some("save"), vec![]), separator(), item("Exit", None, Some("exit"), vec![])]))
        .with_menu(item("Preferences", Some("preferences"), None, vec![item("Settings", None, Some("edit_settings"), vec![])]))
        .with_menu(item("Help", Some("help"), None, vec![item("About CodeLibre Text", None, Some("show_about_window"), vec![])]))
        .with_binding(Binding { keys: vec![KeyCombo::parse("super+s").unwrap()], command: Command::new("save"), context: vec![] });
    let menu = menu::present_for_macos(&World::editor(&world));

    assert_eq!(menu.about.unwrap().command, Some(Command::new("show_about_window")));
    assert_eq!(menu.preferences.unwrap().children[0].caption, "Settings");
    let quit = menu.quit.unwrap();
    assert_eq!((quit.caption.as_str(), quit.command), ("Quit CodeLibre Text", Some(Command::new("exit"))));
    let captions: Vec<&str> = menu.menus.iter().map(|m| m.caption.as_str()).collect();
    assert_eq!(captions, ["File"], "Help had only About, Preferences moved");
    let file: Vec<&str> = menu.menus[0].children.iter().map(|e| e.caption.as_str()).collect();
    assert_eq!(file, ["Save"], "no separator left where Exit was");
    assert_eq!(menu.menus[0].children[0].key, KeyCombo::parse("super+s"));
}

#[test]
fn the_empty_window_shows_the_keys_of_the_keymap() {
    let world = World::new()
        .with_binding(Binding { keys: vec![KeyCombo::parse("super+n").unwrap()], command: Command::new("new_file"), context: vec![] })
        .with_binding(Binding {
            keys: vec![KeyCombo::parse("super+k").unwrap(), KeyCombo::parse("super+o").unwrap()],
            command: Command::new("prompt_open_folder"),
            context: vec![],
        });
    let hints = editor_presenter::present_hints(&World::editor(&world));
    let n = KeyCombo::parse("super+n").unwrap().display();
    assert_eq!(hints.len(), 2, "commands without a key are left out: {hints:?}");
    assert_eq!(hints[0], format!("{n}  New File"));
    assert!(hints[1].ends_with("  Open Folder") && hints[1].contains(", "), "{}", hints[1]);
}

#[test]
fn the_menu_and_the_status_bar_are_shown_in_the_interface_language() {
    let world = World::new()
        .with_language(
            "es",
            "Español",
            &[("File", "Archivo"), ("Save", "Guardar"), ("Line {line}, Column {column}", "Línea {line}, columna {column}")],
        )
        .with_preferences(json!({ "language": "es" }))
        .with_menu(MenuItem {
            caption: "File".into(),
            id: Some("file".into()),
            command: None,
            checkbox: false,
            children: vec![MenuItem {
                caption: "Save".into(),
                id: None,
                command: Some(Command::new("save")),
                checkbox: false,
                children: vec![],
            }],
        });
    let editor = editor_with_text(&world, "ab", Selection::caret(1));
    let menu = menu::present(&editor);
    assert_eq!(menu[0].caption, "Archivo");
    assert_eq!(menu[0].children[0].caption, "Guardar");
    assert_eq!(status_bar::present(&editor).position, "Línea 1, columna 2");
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
fn the_minimap_follows_toggle_minimap_and_its_settings() {
    let world = World::new().with_preferences(json!({ "draw_minimap_border": true }));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let minimap = editor_presenter::present(&mut editor, &mut view).unwrap().minimap;
    assert_eq!(minimap, Some(MinimapViewModel { always_show_viewport: false, draw_border: true }));
    view.show_minimap = false;
    assert_eq!(editor_presenter::present(&mut editor, &mut view).unwrap().minimap, None);
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

#[test]
fn the_output_panel_shows_the_end_of_its_text() {
    let world = World::new().with_plugin_commands(&[("build", use_cases::gateways::PluginCommandKind::Window)]);
    let mut editor = world.editor();
    assert!(output_panel::present(&editor).is_none(), "nothing shown");
    let text: String = (0..output_panel::MAX_LINES + 5).map(|i| format!("line {i}\n")).collect();
    // The panel gets the next document id.
    let panel_id = editor.active_document().unwrap().id + 1;
    world.plugins.then([
        test_support::plugin_call("window_create_output_panel", json!([1, "exec"])),
        test_support::plugin_call("run_command", json!(["view", panel_id, "append", {"characters": text}])),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("build"), &mut |_, _| {});
    editor.show_output_panel("exec");
    let panel = output_panel::present(&editor).unwrap();
    assert_eq!(panel.name, "exec");
    assert_eq!(panel.text.lines().count(), output_panel::MAX_LINES);
    assert!(panel.text.starts_with("line 5\n") && panel.text.ends_with(&format!("line {}\n", output_panel::MAX_LINES + 4)));
    assert_eq!(panel.highlight, None, "plain text is drawn in one color");
    assert!(panel.selections.is_empty());
}

#[test]
fn an_output_panel_with_a_syntax_is_highlighted_and_shows_its_selections() {
    let world = World::new().with_plugin_commands(&[("lint", use_cases::gateways::PluginCommandKind::Window)]);
    let mut editor = world.editor();
    let text: String = (0..output_panel::MAX_LINES + 1).map(|i| format!("ln {i}\n")).collect();
    let panel_id = editor.active_document().unwrap().id + 1;
    let total = text.chars().count() as u64;
    world.plugins.then([
        test_support::plugin_call("window_create_output_panel", json!([1, "lint"])),
        test_support::plugin_call("run_command", json!(["view", panel_id, "append", {"characters": text}])),
        test_support::plugin_call("view_assign_syntax", json!([panel_id, "Packages/Python/Python.sublime-syntax"])),
        // The first line (not shown: past MAX_LINES) and the last two characters.
        test_support::plugin_call("view_selection_set", json!([panel_id, [[0, 3], [total - 3, total - 1], [5, 5]]])),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("lint"), &mut |_, _| {});
    editor.show_output_panel("lint");
    let panel = output_panel::present(&editor).unwrap();
    let highlight = panel.highlight.expect("a syntax was assigned");
    assert_eq!((highlight.document, highlight.syntax.as_str()), (panel_id, "Python"));
    assert_eq!(&highlight.full_text[highlight.start..], panel.text, "the whole text, for the highlighting cache");
    let end = panel.text.len();
    assert_eq!(panel.selections.len(), 1, "carets and what is not shown are left out");
    assert_eq!(panel.selections[0], end - 3..end - 1);
}
#[test]
fn plugin_regions_and_statuses_are_presented() {
    let world = World::new()
        .with_plugin_commands(&[("lint", use_cases::gateways::PluginCommandKind::Text)])
        .with_resource("Packages/Lint/added.png", "png bytes");
    let mut editor = editor_with_text(&world, "one two three", Selection::caret(0));
    let v = editor.active_document().unwrap().id;
    let call = |function: &str, args: serde_json::Value| test_support::plugin_call(function, args);
    world.plugins.then([
        call("view_add_regions", json!([v, "plain", [[8, 13], [0, 3]], "region.bluish", "Packages/Lint/added.png", 0])),
        call("view_add_regions", json!([v, "squiggly", [[4, 7], [5, 5]], "invalid", "dot", 32 | 256 | 2048 | 1 | 2])),
        call("view_add_regions", json!([v, "hidden", [[0, 1]], "", "", 128])),
        call("view_set_status", json!([v, "lint", "1 warning"])),
        call("view_show", json!([v, 8, false])),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("lint"), &mut |_, _| {});

    let model = editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap();
    assert_eq!(model.regions.len(), 2, "hidden regions are not drawn");
    let plain = &model.regions[0];
    assert_eq!(plain.ranges, vec![0..3, 8..13], "sorted");
    assert!(plain.fill && plain.outline && plain.underline.is_none() && !plain.draw_empty);
    assert_eq!(plain.icon_image.as_deref().map(Vec::as_slice), Some(&b"png bytes"[..]), "a resource icon comes with its picture");
    let squiggly = &model.regions[1];
    assert!(squiggly.icon_image.is_none(), "a dot is drawn");
    assert_eq!((squiggly.scope.as_str(), squiggly.icon.as_str()), ("invalid", "dot"));
    assert!(!squiggly.fill && !squiggly.outline && squiggly.draw_empty);
    assert!(plain.on_minimap && !squiggly.on_minimap, "HIDE_ON_MINIMAP");
    assert_eq!(squiggly.underline, Some(interface_adapters::view_models::Underline::Squiggly));
    assert_eq!(model.show, Some((8, false, 1)));
    assert_eq!(status_bar::present(&editor).plugin_statuses, vec!["1 warning".to_string()]);
}

#[test]
fn region_annotations_are_presented_and_their_links_reach_the_plugin() {
    use interface_adapters::controllers::command_controller;
    let world = World::new().with_plugin_commands(&[("lint", use_cases::gateways::PluginCommandKind::Text)]);
    let mut editor = editor_with_text(&world, "one\ntwo three", Selection::caret(0));
    let v = editor.active_document().unwrap().id;
    world.plugins.then([
        test_support::plugin_call(
            "view_add_regions",
            json!([v, "lint", [[8, 13], [0, 3]], "invalid", "", 0, ["<a href='fix'>fix</a> it", "first"], "#ff0000", 9]),
        ),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("lint"), &mut |_, _| {});
    let model = editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap();
    let lint = &model.regions[0];
    let points: Vec<usize> = lint.annotations.iter().map(|a| a.point).collect();
    assert_eq!(points, [0, 8], "by position, each with its own region's annotation");
    assert_eq!(lint.annotations[1].lines[0][0].link.as_deref(), Some("fix"));
    assert_eq!((lint.key.as_str(), lint.annotation_color.as_str()), ("lint", "#ff0000"));

    world.plugins.then([use_cases::gateways::PluginMessage::Done { error: None }]);
    command_controller::navigate_plugin_annotation(&mut editor, &mut ViewState::default(), v, "lint", "fix");
    assert_eq!(world.plugins.callbacks.borrow().last().cloned(), Some((9, "navigate".to_string(), json!(["fix"]))));
}

#[test]
fn phantoms_say_their_layout_and_bring_their_images() {
    use interface_adapters::view_models::PhantomLayout;
    let world = World::new()
        .with_plugin_commands(&[("show", use_cases::gateways::PluginCommandKind::Text)])
        .with_resource("Packages/P/dot.png", vec![0x89, b'P', b'N', b'G']);
    let mut editor = editor_with_text(&world, "one\ntwo", Selection::caret(0));
    let v = editor.active_document().unwrap().id;
    world.plugins.then([
        test_support::plugin_call("view_add_phantom", json!([v, "k", 1, 2, "<b>inline</b>", 0, null])),
        test_support::plugin_call("view_add_phantom", json!([v, "k", 0, 3, "<img src=\"res://Packages/P/dot.png\">", 1, null])),
        test_support::plugin_call("view_add_phantom", json!([v, "k", 4, 4, "<div style='padding: 2px'>block</div>", 2, null])),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("show"), &mut |_, _| {});
    let model = editor_presenter::present(&mut editor, &mut ViewState::default()).unwrap();
    let layouts: Vec<(usize, PhantomLayout)> = model.phantoms.iter().map(|p| (p.point, p.layout)).collect();
    assert_eq!(layouts, [(1, PhantomLayout::Inline), (3, PhantomLayout::Below), (4, PhantomLayout::Block)], "an inline one at its start, the others at their end");
    let below = &model.phantoms[1];
    assert_eq!(below.html.images, vec![("res://Packages/P/dot.png".to_string(), std::rc::Rc::new(vec![0x89, b'P', b'N', b'G']))]);
    let Some(entities::minihtml::Content::Block(block)) = model.phantoms[2].html.block.content.first() else { panic!("a block") };
    assert_eq!(block.style.padding.left, 2.0);
}

#[test]
fn plugin_commands_decide_how_they_show_in_the_menu() {
    let item = |caption: &str, command: &str, checkbox: bool| MenuItem {
        caption: caption.into(),
        id: None,
        command: Some(Command::new(command)),
        checkbox,
        children: vec![],
    };
    let world = World::new()
        .with_menu(MenuItem {
            caption: "Tools".into(),
            id: None,
            command: None,
            checkbox: false,
            children: vec![item("Hidden", "hidden", false), item("Off", "off", true), item("", "described", false)],
        })
        .with_plugins(
            &[
                ("hidden", use_cases::gateways::PluginCommandKind::Window),
                ("off", use_cases::gateways::PluginCommandKind::Window),
                ("described", use_cases::gateways::PluginCommandKind::Window),
            ],
            &["command_states"],
        );
    let mut editor = world.editor();
    world.plugins.then([use_cases::gateways::PluginMessage::Answer(json!([
        { "visible": false },
        { "enabled": false, "checked": true },
        { "description": "From the plugin" },
    ]))]);
    editor.refresh_plugin_command_states();
    let menu = menu::present(&editor);
    let tools = &menu[0].children;
    assert_eq!(tools.len(), 2, "the hidden one is left out");
    assert!(!tools[0].enabled);
    assert_eq!(tools[0].checked, Some(true));
    assert_eq!(tools[1].caption, "From the plugin");
    assert!(tools[1].enabled);
}

#[test]
fn a_plugins_quick_panel_shows_kinds_and_may_stay_open() {
    use interface_adapters::controllers::command_controller;
    use interface_adapters::presenters::quick_panel;
    use interface_adapters::view_state::ViewState;
    use test_support::plugin_call;
    use use_cases::gateways::{PluginCommandKind, PluginMessage};

    let world = World::new().with_plugin_commands(&[("pick", PluginCommandKind::Window)]);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    world.plugins.then([
        plugin_call("window_show_quick_panel", json!([1, [["one", "", "note", "f"]], 2, -1, "", 7, false])),
        PluginMessage::Done { error: None },
    ]);
    command_controller::run(&mut editor, &mut view, &Command::new("pick"));
    let panel = quick_panel::present(&editor, &mut view).expect("the panel");
    assert!(panel.keep_open);
    assert_eq!(panel.items[0].kind, "f");
    assert_eq!(panel.items[0].hint, "note");
}

#[test]
fn a_plugins_quick_panel_keeps_each_detail_line_and_its_links() {
    use interface_adapters::controllers::{command_controller, ui_controller};
    use interface_adapters::presenters::quick_panel;
    use interface_adapters::view_state::ViewState;
    use test_support::plugin_call;
    use use_cases::gateways::{PluginCommandKind, PluginMessage};

    let world = World::new().with_plugin_commands(&[("pick", PluginCommandKind::Window)]);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    // As Package Control lists a package, then a row of plain lines.
    let package = json!(["1337", ["<em>A &amp; B</em>", "<em>install v2</em> <a href=\"https://x.test/y\">x.test/y</a>"], "", "", true]);
    let plain = json!(["caption", ["<b>kept</b>", "second"], "", "", false]);
    world.plugins.then([plugin_call("window_show_quick_panel", json!([1, [package, plain], 0, -1, "", 7, false])), PluginMessage::Done { error: None }]);
    command_controller::run(&mut editor, &mut view, &Command::new("pick"));
    let panel = quick_panel::present(&editor, &mut view).expect("the panel");

    let details = &panel.items[0].details;
    assert_eq!(details.len(), 2);
    assert_eq!((details[0][0].text.as_str(), details[0][0].italic), ("A & B", true));
    let link = details[1].iter().find(|s| s.link.is_some()).expect("a link");
    assert_eq!((link.text.trim(), link.link.as_deref()), ("x.test/y", Some("https://x.test/y")));
    let texts: Vec<&str> = panel.items[1].details.iter().map(|line| line[0].text.as_str()).collect();
    assert_eq!(texts, ["<b>kept</b>", "second"], "a list's lines are plain text");

    // The query matches the text of the details, and a link opens in the browser.
    assert_eq!(editor.search(use_cases::search::SearchKind::PluginItems, "install").len(), 1);
    ui_controller::handle(&mut editor, &mut view, ui_controller::UiEvent::QuickPanelLinkClicked("https://x.test/y".into()));
    assert_eq!(*world.opened_urls.borrow(), ["https://x.test/y"]);
    assert!(view.quick_panel.is_some(), "the panel stays open");
}

#[test]
fn the_output_panel_marks_the_result_visited_relative_to_the_lines_it_shows() {
    let world = World::new().with_file("/p/a.py", "x\n");
    let mut editor = world.editor();
    editor.exec(&json!({ "cmd": ["x"], "file_regex": "^(a.py):(\\d+)", "working_dir": "/p" }));
    editor.show_output_panel("exec");
    let mut text: String = (0..output_panel::MAX_LINES + 5).map(|i| format!("line {i}\n")).collect();
    text.push_str("a.py:1\n");
    world.processes.events.borrow_mut().push_back(use_cases::gateways::ProcessEvent::Output(text));
    editor.poll_builds();

    let panel = output_panel::present(&editor).unwrap();
    assert_eq!(panel.first_line, 6);
    assert_eq!(panel.current_line, None);
    let revision = panel.result_revision;

    assert!(editor.next_result(true));
    let panel = output_panel::present(&editor).unwrap();
    assert_eq!(panel.current_line, Some(output_panel::MAX_LINES - 1));
    assert_ne!(panel.result_revision, revision);
}

#[test]
fn the_input_panel_shows_its_document() {
    use interface_adapters::view_state::InputPanelState;
    let world = World::new().with_plugin_commands(&[("ask", use_cases::gateways::PluginCommandKind::Window)]);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(quick_panel::present_input_panel(&editor, &mut view).is_none());
    world.plugins.then([
        test_support::plugin_call("window_show_input_panel", json!([1, "Name:", "bob", 3, false])),
        use_cases::gateways::PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&entities::command::Command::new("ask"), &mut |_, _| {});
    view.input_panel = Some(InputPanelState { focus_requested: true });

    let panel = quick_panel::present_input_panel(&editor, &mut view).unwrap();
    assert_eq!((panel.caption.as_str(), panel.text.as_str(), panel.selection), ("Name:", "bob", (3, 3)));
    assert!(panel.take_focus);
    assert!(!quick_panel::present_input_panel(&editor, &mut view).unwrap().take_focus, "only once");
}
