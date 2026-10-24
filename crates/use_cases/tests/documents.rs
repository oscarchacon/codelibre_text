//! Tests for the document use cases (`use_cases::documents`): open, switch, close, reload.

use std::path::{Path, PathBuf};
use std::time::Duration;

use entities::document::LineEnding;
use entities::selection::Selection;
use use_cases::editing::TextCommand;
use use_cases::editor::{CloseChoice, PendingClose};
use test_support::{active_text, open, status, World};

fn names(editor: &use_cases::Editor) -> Vec<String> {
    editor.state().documents.iter().map(|d| d.name()).collect()
}

// ----- new and open --------------------------------------------------------------------------------

#[test]
fn new_file_adds_an_untitled_tab_and_activates_it() {
    let world = World::new();
    let mut editor = world.editor();
    editor.new_file();
    assert_eq!(names(&editor), vec!["untitled", "untitled"]);
    assert_eq!(editor.state().active, 1);
}

#[test]
fn new_files_use_the_default_line_ending_setting() {
    let world = World::new().with_preferences(serde_json::json!({ "default_line_ending": "windows" }));
    let editor = world.editor();
    assert_eq!(editor.active_document().unwrap().line_ending, LineEnding::Windows);
}

#[test]
fn opening_a_file_replaces_the_pristine_untitled_tab() {
    let world = World::new().with_file("/p/app.py", "x = 1\n");
    let mut editor = world.editor();
    assert_eq!(open(&mut editor, "/p/app.py"), "x = 1\n");
    assert_eq!(names(&editor), vec!["app.py"]);
    assert_eq!(editor.active_document().unwrap().syntax, "Python");
}

#[test]
fn opening_keeps_a_modified_untitled_tab() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("draft".into()));
    open(&mut editor, "/p/a.txt");
    assert_eq!(names(&editor), vec!["draft", "a.txt"]);
    assert_eq!(editor.state().active, 1);
}

#[test]
fn opening_an_open_file_just_activates_it() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    open(&mut editor, "/p/a.txt");
    assert_eq!(names(&editor), vec!["a.txt", "b.txt"]);
    assert_eq!(editor.state().active, 0);
}

#[test]
fn opening_a_missing_path_starts_an_empty_buffer_for_it() {
    let world = World::new();
    let mut editor = world.editor();
    assert!(editor.open_file(Path::new("/p/new.rs")));
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.path, Some(PathBuf::from("/p/new.rs")));
    assert_eq!(doc.syntax, "Rust");
    assert!(doc.text.is_empty());
}

#[test]
fn binary_files_are_not_opened() {
    let world = World::new().with_file("/p/app.exe", vec![b'M', b'Z', 0, 0]);
    let mut editor = world.editor();
    assert!(!editor.open_file(Path::new("/p/app.exe")));
    assert!(status(&editor).contains("binary"));
    assert_eq!(names(&editor), vec!["untitled"]);
}

#[test]
fn unreadable_files_report_the_error() {
    let world = World::new().with_dir("/p/folder");
    let mut editor = world.editor();
    assert!(!editor.open_file(Path::new("/p/folder")));
    assert!(status(&editor).contains("folder"));
}

#[test]
fn prompt_open_files_opens_every_picked_file_starting_in_the_current_folder() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    *world.dialogs.files_to_open.borrow_mut() = vec![PathBuf::from("/p/b.txt")];
    editor.prompt_open_files();
    assert_eq!(names(&editor), vec!["a.txt", "b.txt"]);
    assert_eq!(*world.dialogs.last_open_dir.borrow(), Some(PathBuf::from("/p")));
}

#[test]
fn scratch_buffers_open_once() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_scratch("Defaults", "{}", "JSON", true);
    editor.new_file();
    editor.open_scratch("Defaults", "{}", "JSON", true);
    assert_eq!(names(&editor), vec!["untitled", "Defaults", "untitled"]);
    assert_eq!(editor.state().active, 1);
    assert!(editor.active_document().unwrap().read_only);
}

// ----- switching -----------------------------------------------------------------------------------

#[test]
fn activate_ignores_indexes_out_of_range() {
    let world = World::new();
    let mut editor = world.editor();
    editor.activate(5);
    assert_eq!(editor.state().active, 0);
}

#[test]
fn cycling_documents_wraps_around() {
    let world = World::new();
    let mut editor = world.editor();
    editor.new_file();
    editor.new_file();
    editor.activate(2);
    editor.cycle_documents(true);
    assert_eq!(editor.state().active, 0);
    editor.cycle_documents(false);
    assert_eq!(editor.state().active, 2);
}

#[test]
fn cycling_recent_documents_goes_back_to_the_previous_one() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b").with_file("/p/c.txt", "c");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    open(&mut editor, "/p/c.txt");
    editor.activate(0);
    editor.cycle_recent_documents(true);
    assert_eq!(editor.active_document().unwrap().name(), "c.txt");
    editor.cycle_recent_documents(false);
    assert_eq!(editor.active_document().unwrap().name(), "b.txt");
}

#[test]
fn activate_document_by_id() {
    let world = World::new();
    let mut editor = world.editor();
    let first = editor.active_document().unwrap().id;
    editor.new_file();
    editor.activate_document(first);
    assert_eq!(editor.state().active, 0);
}

// ----- closing -------------------------------------------------------------------------------------

#[test]
fn closing_a_clean_document_removes_it() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let id = editor.active_document().unwrap().id;
    editor.close_active_document();
    assert!(editor.state().documents.is_empty());
    assert!(world.syntaxes.forgotten.borrow().contains(&id), "highlighting cache released");
}

#[test]
fn closing_a_dirty_document_asks_first() {
    let world = World::new();
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("draft".into()));
    let id = editor.active_document().unwrap().id;
    editor.close_active_document();
    assert_eq!(editor.state().pending_close, Some(PendingClose::Documents(vec![id])));
    assert_eq!(editor.state().documents.len(), 1);
}

#[test]
fn cancel_keeps_the_document() {
    let world = World::new();
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("draft".into()));
    editor.close_active_document();
    assert!(!editor.resolve_pending_close(CloseChoice::Cancel));
    assert_eq!(editor.state().pending_close, None);
    assert_eq!(editor.state().documents.len(), 1);
}

#[test]
fn dont_save_closes_without_writing() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("x".into()));
    editor.close_active_document();
    editor.resolve_pending_close(CloseChoice::DontSave);
    assert!(editor.state().documents.is_empty());
    assert_eq!(world.file_text("/p/a.txt"), "a");
}

#[test]
fn save_writes_then_closes() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("x".into()));
    editor.close_active_document();
    editor.resolve_pending_close(CloseChoice::Save);
    assert!(editor.state().documents.is_empty());
    assert_eq!(world.file_text("/p/a.txt"), "xa");
}

#[test]
fn close_all_asks_about_each_dirty_document_in_turn() {
    let world = World::new();
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("one".into()));
    editor.new_file();
    editor.run_text_command(TextCommand::Insert("two".into()));
    let ids: Vec<u64> = editor.state().documents.iter().map(|d| d.id).collect();

    editor.close_all_documents();
    assert_eq!(editor.state().pending_close, Some(PendingClose::Documents(ids.clone())));
    editor.resolve_pending_close(CloseChoice::DontSave);
    assert_eq!(editor.state().pending_close, Some(PendingClose::Documents(vec![ids[1]])));
    editor.resolve_pending_close(CloseChoice::DontSave);
    assert!(editor.state().documents.is_empty());
}

#[test]
fn closing_activates_the_most_recently_used_document() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b").with_file("/p/c.txt", "c");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    open(&mut editor, "/p/c.txt");
    editor.activate(0);
    editor.activate(2);
    editor.close_active_document();
    assert_eq!(editor.active_document().unwrap().name(), "a.txt");
}

#[test]
fn reopen_closed_file_brings_back_the_last_one() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    editor.close_active_document();
    editor.reopen_closed_file();
    assert_eq!(editor.active_document().unwrap().name(), "b.txt");
    editor.reopen_closed_file();
    assert_eq!(editor.state().documents.len(), 2, "nothing left to reopen");
}

// ----- syntax, line endings, revert ----------------------------------------------------------------

#[test]
fn set_syntax_accepts_names_and_sublime_syntax_paths() {
    let world = World::new();
    let mut editor = world.editor();
    editor.set_syntax("Packages/Python/Python.sublime-syntax");
    assert_eq!(editor.active_document().unwrap().syntax, "Python");
    editor.set_syntax("Rust");
    assert_eq!(editor.active_document().unwrap().syntax, "Rust");
    editor.set_syntax("Cobol");
    assert_eq!(editor.active_document().unwrap().syntax, "Plain Text");
}

#[test]
fn changing_line_endings_marks_the_file_as_modified() {
    let world = World::new().with_file("/p/a.txt", "a\n");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.set_line_ending(LineEnding::Windows);
    assert!(editor.active_document().unwrap().is_dirty());
}

#[test]
fn revert_discards_changes() {
    let world = World::new().with_file("/p/a.txt", "saved");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("junk".into()));
    editor.revert_active_document();
    assert_eq!(active_text(&editor), "saved");
    assert!(!editor.active_document().unwrap().is_dirty());
}

#[test]
fn files_changed_by_other_programs_are_reloaded_unless_modified_here() {
    let world = World::new().with_file("/p/clean.txt", "old").with_file("/p/dirty.txt", "old");
    let mut editor = world.editor();
    open(&mut editor, "/p/clean.txt");
    open(&mut editor, "/p/dirty.txt");
    editor.run_text_command(TextCommand::Insert("mine ".into()));

    world.disk.put("/p/clean.txt", "new");
    world.disk.put("/p/dirty.txt", "new");
    // The check runs at most every 1.5 seconds.
    std::thread::sleep(Duration::from_millis(1600));
    editor.check_external_changes();

    let docs = &editor.state().documents;
    assert_eq!(docs[0].text, "new");
    assert_eq!(docs[1].text, "mine old");
    assert!(status(&editor).contains("changed on disk"));
}

// ----- the view ------------------------------------------------------------------------------------

#[test]
fn the_view_reports_the_caret_without_bumping_the_revision() {
    let world = World::new().with_file("/p/a.txt", "hello");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let doc = editor.active_document().unwrap();
    let (id, revision) = (doc.id, doc.selection_revision());
    editor.sync_selection_from_view(id, Selection::new(1, 3));
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::new(1, 3));
    assert_eq!(doc.selection_revision(), revision);
}

#[test]
fn the_text_widget_can_edit_documents_but_not_read_only_ones() {
    let world = World::new();
    let mut editor = world.editor();
    let id = editor.active_document().unwrap().id;
    editor.text_for_editing(id).unwrap().push_str("typed");
    assert_eq!(active_text(&editor), "typed");

    editor.open_scratch("Defaults", "{}", "JSON", true);
    let scratch = editor.active_document().unwrap().id;
    assert!(editor.text_for_editing(scratch).is_none());
}
