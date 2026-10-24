//! Tests for hot exit and quitting (`use_cases::session`).

use std::path::PathBuf;

use entities::selection::Selection;
use serde_json::json;
use test_support::{open, World};
use use_cases::editing::TextCommand;
use use_cases::editor::{CloseChoice, PendingClose};
use use_cases::session::{Session, SessionDocument};

fn layout() -> Vec<(String, bool)> {
    vec![("side_bar".into(), false)]
}

fn saved(world: &World) -> Session {
    world.session.borrow().clone().expect("a saved session")
}

// ----- quitting ------------------------------------------------------------------------------------

#[test]
fn with_hot_exit_quitting_saves_the_session_without_asking() {
    let world = World::new();
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("unsaved".into()));
    assert!(editor.request_exit(layout()));
    assert_eq!(saved(&world).layout, layout());
    assert!(editor.state().pending_close.is_none());
}

#[test]
fn without_hot_exit_unsaved_changes_are_asked_about() {
    let world = World::new().with_preferences(json!({ "hot_exit": false }));
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("unsaved".into()));
    assert!(!editor.request_exit(layout()));
    assert_eq!(editor.state().pending_close, Some(PendingClose::Exit));
    assert!(editor.resolve_pending_close(CloseChoice::DontSave));
}

#[test]
fn cancelling_the_exit_question_keeps_running() {
    let world = World::new().with_preferences(json!({ "hot_exit": false }));
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("unsaved".into()));
    editor.request_exit(layout());
    assert!(!editor.resolve_pending_close(CloseChoice::Cancel));
}

#[test]
fn without_hot_exit_and_without_changes_quitting_is_immediate() {
    let world = World::new().with_preferences(json!({ "hot_exit": false }));
    let mut editor = world.editor();
    assert!(editor.request_exit(layout()));
}

// ----- saving the session --------------------------------------------------------------------------

#[test]
fn the_session_keeps_folders_files_unsaved_text_and_carets() {
    let world = World::new().with_dir("/p").with_file("/p/clean.txt", "c").with_file("/p/dirty.txt", "d");
    let mut editor = world.editor_opening(&["/p"]);
    open(&mut editor, "/p/clean.txt");
    open(&mut editor, "/p/dirty.txt");
    editor.run_text_command(TextCommand::Insert("x".into()));
    editor.save_session(layout());

    let session = saved(&world);
    assert_eq!(session.folders, vec![PathBuf::from("/p")]);
    assert_eq!(session.active, 1);
    assert_eq!(session.documents.len(), 2);
    assert_eq!(session.documents[0].unsaved_text, None, "clean files are re-read from disk");
    assert_eq!(session.documents[1].unsaved_text.as_deref(), Some("xd"));
    assert_eq!(session.documents[1].selection, Selection::caret(1));
}

#[test]
fn empty_untitled_tabs_and_read_only_views_are_not_saved() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_scratch("Defaults", "{}", "JSON", true);
    editor.new_file();
    editor.run_text_command(TextCommand::Insert("draft".into()));
    editor.save_session(layout());

    let session = saved(&world);
    assert_eq!(session.documents.len(), 1);
    assert_eq!(session.documents[0].path, None);
    assert_eq!(session.documents[0].unsaved_text.as_deref(), Some("draft"));
}

// ----- restoring the session -----------------------------------------------------------------------

fn session_document(path: Option<&str>, unsaved: Option<&str>, selection: Selection) -> SessionDocument {
    SessionDocument {
        path: path.map(PathBuf::from),
        unsaved_text: unsaved.map(String::from),
        syntax: "Python".into(),
        selection,
    }
}

#[test]
fn startup_restores_the_previous_session() {
    let world = World::new().with_dir("/p").with_file("/p/a.py", "saved").with_session(Session {
        folders: vec![PathBuf::from("/p"), PathBuf::from("/gone")],
        documents: vec![
            session_document(Some("/p/a.py"), None, Selection::caret(2)),
            session_document(Some("/p/a.py"), Some("changed"), Selection::caret(1)),
            session_document(Some("/p/deleted.py"), Some("kept"), Selection::caret(0)),
            session_document(None, Some("scratch notes"), Selection::caret(0)),
        ],
        active: 3,
        layout: layout(),
    });
    let editor = world.editor();
    let state = editor.state();

    assert_eq!(state.folders, vec![PathBuf::from("/p")], "missing folders are skipped");
    let texts: Vec<&str> = state.documents.iter().map(|d| d.text.as_str()).collect();
    assert_eq!(texts, vec!["saved", "changed", "kept", "scratch notes"]);
    assert!(!state.documents[0].is_dirty());
    assert!(state.documents[1].is_dirty());
    assert_eq!(state.documents[0].selection(), Selection::caret(2));
    assert_eq!(state.documents[2].path, Some(PathBuf::from("/p/deleted.py")));
    assert_eq!(state.documents[0].syntax, "Python");
    assert_eq!(state.active, 3);
}

#[test]
fn the_session_is_not_restored_without_hot_exit() {
    let world = World::new()
        .with_preferences(json!({ "hot_exit": false }))
        .with_session(Session { documents: vec![session_document(None, Some("old"), Selection::caret(0))], ..Default::default() });
    let editor = world.editor();
    assert_eq!(editor.state().documents.len(), 1);
    assert!(editor.state().documents[0].text.is_empty());
}

#[test]
fn session_layout_returns_the_saved_ui_flags() {
    let world = World::new().with_session(Session { layout: layout(), ..Default::default() });
    let editor = world.editor();
    assert_eq!(editor.session_layout(), layout());
}

#[test]
fn session_layout_is_empty_without_a_session() {
    let world = World::new();
    let editor = world.editor();
    assert!(editor.session_layout().is_empty());
}
