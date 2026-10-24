//! Tests for the quick panel searches (`use_cases::search`).

use std::path::PathBuf;

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use entities::menu::PaletteCommand;
use entities::selection::Selection;
use entities::symbol::Symbol;
use serde_json::json;
use test_support::{open, World};
use use_cases::search::{CandidateTarget, SearchKind};

fn captions(candidates: &[use_cases::search::Candidate]) -> Vec<String> {
    candidates.iter().map(|c| c.caption.clone()).collect()
}

fn palette_world() -> World {
    World::new()
        .with_palette(PaletteCommand { caption: "File: Save".into(), command: Command::new("save") })
        .with_palette(PaletteCommand { caption: "View: Toggle Side Bar".into(), command: Command::new("toggle_side_bar") })
        .with_binding(Binding { keys: vec![KeyCombo::parse("ctrl+s").unwrap()], command: Command::new("save"), context: vec![] })
}

// ----- Command Palette -----------------------------------------------------------------------------

#[test]
fn the_palette_lists_entries_with_their_key_binding() {
    let world = palette_world();
    let editor = world.editor();
    let results = editor.search(SearchKind::Commands, "save");
    assert_eq!(results[0].caption, "File: Save");
    assert_eq!(results[0].hint, "Ctrl+S");
    assert_eq!(results[0].target, CandidateTarget::Command(Command::new("save")));
}

#[test]
fn the_palette_offers_set_syntax_for_every_syntax() {
    let world = palette_world();
    let editor = world.editor();
    let all = captions(&editor.search(SearchKind::Commands, ""));
    assert!(all.contains(&"Set Syntax: Python".to_string()));
    assert_eq!(all.len(), 2 + 6);

    let python = editor.search(SearchKind::Commands, "Set Syntax: Pyth");
    assert_eq!(python[0].caption, "Set Syntax: Python");
    assert_eq!(
        python[0].target,
        CandidateTarget::Command(Command::with_args("set_file_type", json!({ "syntax": "Python" })))
    );
}

#[test]
fn fuzzy_queries_filter_out_non_matches() {
    let world = palette_world();
    let editor = world.editor();
    let results = captions(&editor.search(SearchKind::Commands, "tsb"));
    assert_eq!(results.first().map(String::as_str), Some("View: Toggle Side Bar"));
    assert!(!results.contains(&"File: Save".to_string()));
}

#[test]
fn color_schemes_are_listed() {
    let world = World::new();
    let editor = world.editor();
    assert_eq!(captions(&editor.search(SearchKind::ColorSchemes, "")), vec!["Daylight", "Monokai", "Solarized (dark)"]);
    assert_eq!(captions(&editor.search(SearchKind::ColorSchemes, "mono")), vec!["Monokai"]);
}

// ----- Goto Anything -------------------------------------------------------------------------------

#[test]
fn colon_goes_to_a_line() {
    let world = World::new();
    let editor = world.editor();
    let results = editor.search(SearchKind::Goto, ":12");
    assert_eq!(captions(&results), vec!["Go to line 12"]);
    assert_eq!(results[0].target, CandidateTarget::Line(11));
    assert_eq!(captions(&editor.search(SearchKind::Goto, ":abc")), vec!["Type a line number"]);
}

#[test]
fn at_lists_the_symbols_of_the_active_document() {
    let world = World::new();
    *world.syntaxes.symbols.borrow_mut() = vec![
        Symbol { line: 0, name: "main".into() },
        Symbol { line: 9, name: "helper".into() },
    ];
    let editor = world.editor();
    let all = editor.search(SearchKind::Goto, "@");
    assert_eq!(captions(&all), vec!["main", "helper"], "file order when there is no query");
    assert_eq!(all[1].hint, "10");
    assert_eq!(all[1].target, CandidateTarget::Line(9));
    assert_eq!(captions(&editor.search(SearchKind::Goto, "@help")), vec!["helper"]);
}

#[test]
fn files_come_from_open_documents_and_the_folder_index() {
    let world = World::new()
        .with_dir("/proj")
        .with_file("/proj/src/main.rs", "")
        .with_indexed_files(&["/proj/src/main.rs", "/proj/src/lib.rs", "/proj/README.md"]);
    let mut editor = world.editor_opening(&["/proj"]);
    open(&mut editor, "/proj/src/main.rs");

    let results = editor.search(SearchKind::Goto, "");
    let main = results.iter().filter(|c| c.caption == "main.rs").count();
    assert_eq!(main, 1, "an open file is listed once");
    let lib = results.iter().find(|c| c.caption == "lib.rs").unwrap();
    let relative = std::path::Path::new("/proj/src/lib.rs").strip_prefix("/proj").unwrap();
    assert_eq!(lib.detail, relative.to_string_lossy(), "shown relative to the open folder");
    assert_eq!(lib.target, CandidateTarget::OpenFile(PathBuf::from("/proj/src/lib.rs")));
    let open_doc = results.iter().find(|c| c.caption == "main.rs").unwrap();
    assert_eq!(open_doc.hint, "open");
}

#[test]
fn file_name_matches_rank_above_folder_matches() {
    let world = World::new()
        .with_dir("/proj")
        .with_indexed_files(&["/proj/readme/notes.txt", "/proj/docs/readme.md"]);
    let editor = world.editor_opening(&["/proj"]);
    let results = editor.search(SearchKind::Goto, "readme");
    assert_eq!(results[0].caption, "readme.md");
}

// ----- choosing a result ---------------------------------------------------------------------------

#[test]
fn accepting_a_command_returns_it_to_the_caller() {
    let world = World::new();
    let mut editor = world.editor();
    let command = Command::new("save");
    assert_eq!(editor.accept_candidate(CandidateTarget::Command(command.clone())), Some(command));
}

#[test]
fn accepting_a_file_opens_it() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    assert_eq!(editor.accept_candidate(CandidateTarget::OpenFile(PathBuf::from("/p/a.txt"))), None);
    assert_eq!(editor.active_document().unwrap().name(), "a.txt");
}

#[test]
fn accepting_a_document_activates_it() {
    let world = World::new();
    let mut editor = world.editor();
    let first = editor.active_document().unwrap().id;
    editor.new_file();
    editor.accept_candidate(CandidateTarget::Document(first));
    assert_eq!(editor.state().active, 0);
}

#[test]
fn accepting_a_line_moves_the_caret() {
    let world = World::new().with_file("/p/a.txt", "a\nb\nc");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.accept_candidate(CandidateTarget::Line(2));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(4));
}

#[test]
fn accepting_a_color_scheme_saves_it_and_previewing_does_not() {
    let world = World::new();
    let mut editor = world.editor();
    editor.preview_candidate(&CandidateTarget::ColorScheme("Daylight".into()));
    assert_eq!(editor.color_scheme_name(), "Daylight");
    assert!(world.resources.user_settings.borrow().is_empty());

    editor.accept_candidate(CandidateTarget::ColorScheme("Daylight".into()));
    assert_eq!(world.resources.user_settings.borrow()["color_scheme"], json!("Daylight.sublime-color-scheme"));
}

#[test]
fn previewing_a_line_moves_the_caret_and_other_previews_do_nothing() {
    let world = World::new().with_file("/p/a.txt", "a\nb");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.preview_candidate(&CandidateTarget::Line(1));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(2));
    editor.preview_candidate(&CandidateTarget::Command(Command::new("exit")));
    assert_eq!(editor.state().documents.len(), 1);
}
