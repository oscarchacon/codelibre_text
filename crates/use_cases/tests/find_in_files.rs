//! Tests for Find in Files (`use_cases::find_in_files`): the search the "Where" field asks for,
//! Find Results in the `find_results` panel as Sublime Text 4215 writes it, going to a result,
//! replacing in the files, and cancelling.

use std::path::{Path, PathBuf};

use entities::command::Command;
use entities::find::FindOptions;
use entities::find_in_files::{RESULT_FILE_REGEX, RESULT_LINE_REGEX};
use entities::selection::Selection;
use serde_json::json;
use test_support::{status, World};
use use_cases::editing::TextCommand;
use use_cases::find_in_files::FIND_RESULTS_PANEL;
use use_cases::gateways::MessageAnswer;
use use_cases::Editor;

const A_TXT: &str = "one\ntwo foo\nthree\nfour\nfive\nsix\nseven\neight foo bar foo\nnine\n";

/// The files of the probe Sublime searched, in the fake home folder (`~/data`).
fn editor_with_data(world: &World) -> Editor {
    world.disk.put("/home/user/data/a.txt", A_TXT);
    world.disk.put("/home/user/data/sub/b.txt", "foo\n");
    world.disk.put("/home/user/data/c.txt", "nothing\n");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/home/user/data"));
    editor
}

/// The home folder as Find Results writes it: `~`, but the whole path on Windows.
const HOME: &str = if cfg!(windows) { "/home/user" } else { "~" };

fn search(editor: &mut Editor, query: &str) {
    editor.set_find_options(FindOptions {
        query: query.to_string(),
        ..FindOptions::default()
    });
}

fn results(editor: &Editor) -> String {
    let panel = editor
        .shown_output_panel()
        .expect("the results panel shows");
    assert_eq!(panel.name, FIND_RESULTS_PANEL);
    panel.document.text.clone()
}

#[test]
fn results_are_written_as_sublime_writes_them() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    assert!(editor.find_in_files(false));
    assert!(editor.is_finding_in_files());
    editor.poll_find_in_files();
    assert!(!editor.is_finding_in_files());
    // What Sublime 4215 wrote for these files.
    assert_eq!(
        results(&editor),
        format!(
            "Searching 3 files for \"foo\"\n\n\
             {HOME}/data/a.txt:\n    1  one\n    2: two foo\n    3  three\n    4  four\n    .\n    6  six\n    7  seven\n    8: eight foo bar foo\n    9  nine\n   10  \n\n\
             {HOME}/data/sub/b.txt:\n    1: foo\n    2  \n\n\
             4 matches across 2 files\n"
        )
    );
    assert_eq!(status(&editor), "4 matches across 2 files");
    let panel = editor.shown_output_panel().unwrap().document.id;
    let settings = editor.settings_of(panel);
    assert_eq!(settings.str("result_file_regex"), Some(RESULT_FILE_REGEX));
    assert_eq!(settings.str("result_line_regex"), Some(RESULT_LINE_REGEX));
}

#[test]
fn the_summary_says_in_one_file_or_no_matches() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "nine");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert!(results(&editor).ends_with("\n\n1 match in 1 file\n"));
    search(&mut editor, "zzz");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert_eq!(
        results(&editor),
        "Searching 3 files for \"zzz\"\n\n0 matches\n"
    );
}

#[test]
fn the_header_names_the_options() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_options(FindOptions {
        query: "F.o".into(),
        regex: true,
        case_sensitive: true,
        whole_word: true,
        wrap: true,
    });
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert_eq!(
        results(&editor),
        "Searching 3 files for \"F.o\" (regex, case sensitive, whole word)\n\n0 matches\n"
    );
}

#[test]
fn next_result_opens_each_match() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert!(editor.next_result(true));
    let doc = editor.active_document().unwrap();
    assert_eq!(
        doc.path.as_deref(),
        Some(Path::new("/home/user/data/a.txt"))
    );
    assert_eq!(
        doc.selection(),
        Selection::new(8, 11),
        "the match on line 2 is selected"
    );
    assert!(editor.next_result(true));
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(44, 47),
        "the first match on line 8"
    );
    assert!(editor.next_result(true));
    assert_eq!(
        editor.active_document().unwrap().path.as_deref(),
        Some(Path::new("/home/user/data/sub/b.txt"))
    );
}

#[test]
fn where_lists_files_folders_and_patterns() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    world.disk.put("/elsewhere/x.md", "foo\n");
    world.disk.put("/elsewhere/x.rs", "foo\n");
    search(&mut editor, "foo");
    editor.set_find_where("/home/user/data/a.txt, /elsewhere, -*.rs".into());
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let request = world.file_search.requests.borrow()[0].clone();
    assert_eq!(
        request.roots,
        vec![
            PathBuf::from("/home/user/data/a.txt"),
            PathBuf::from("/elsewhere")
        ]
    );
    assert_eq!(request.filter.exclude, vec!["*.rs".to_string()]);
    // The settings of the fake world.
    assert_eq!(request.filter.folder_excludes, vec![".git", "target"]);
    assert_eq!(request.filter.file_excludes, vec!["*.pyc"]);
    assert_eq!(request.context_lines, 2);
    let text = results(&editor);
    assert!(text.starts_with("Searching 2 files for"), "{text}");
    assert!(text.contains("/elsewhere/x.md:\n"));
    assert!(!text.contains("x.rs"));
}

#[test]
fn a_relative_or_home_folder_in_where() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    editor.set_find_where("sub, ~/data/c.txt".into());
    editor.find_in_files(false);
    let roots = world.file_search.requests.borrow()[0].roots.clone();
    assert_eq!(
        roots,
        vec![
            PathBuf::from("/home/user/data/sub"),
            PathBuf::from("/home/user/data/c.txt")
        ]
    );
}

#[test]
fn open_files_are_searched_as_they_are_in_the_editor() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    assert!(editor.open_file(Path::new("/home/user/data/c.txt")));
    editor.run_text_command(TextCommand::Insert("foo ".into()));
    search(&mut editor, "foo");
    editor.set_find_where("<open files>".into());
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert!(
        results(&editor).contains(&format!("{HOME}/data/c.txt:\n    1: foo nothing\n")),
        "{}",
        results(&editor)
    );
}

#[test]
fn replacing_asks_then_edits_each_file_unsaved() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    editor.set_replacement("bar".into());
    world
        .dialogs
        .message_answers
        .borrow_mut()
        .push_back(MessageAnswer::Ok);
    editor.find_in_files(true);
    editor.poll_find_in_files();
    let asked = world.dialogs.messages.borrow()[0].text.clone();
    assert_eq!(asked, "Replace 4 occurrences across 2 files?");
    let text_of = |editor: &Editor, path: &str| {
        editor
            .state()
            .documents
            .iter()
            .find(|d| d.path.as_deref() == Some(Path::new(path)))
            .map(|d| (d.text.clone(), d.is_dirty()))
    };
    let (a, dirty) = text_of(&editor, "/home/user/data/a.txt").unwrap();
    assert_eq!(a, A_TXT.replace("foo", "bar"));
    assert!(dirty, "left unsaved");
    assert_eq!(world.file_text("/home/user/data/a.txt"), A_TXT);
    assert_eq!(
        text_of(&editor, "/home/user/data/sub/b.txt").unwrap().0,
        "bar\n"
    );
    assert_eq!(status(&editor), "Replaced 4 occurrences");
}

#[test]
fn replacing_does_nothing_when_the_user_cancels() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    editor.set_replacement("bar".into());
    editor.find_in_files(true);
    editor.poll_find_in_files();
    assert_eq!(world.dialogs.messages.borrow().len(), 1);
    assert!(editor
        .state()
        .documents
        .iter()
        .all(|d| d.path.as_deref() != Some(Path::new("/home/user/data/a.txt"))));
}

#[test]
fn nothing_starts_without_a_query_a_valid_regex_or_a_place() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    assert!(!editor.find_in_files(false), "empty query");
    editor.set_find_options(FindOptions {
        query: "(".into(),
        regex: true,
        ..FindOptions::default()
    });
    assert!(!editor.find_in_files(false));
    assert!(editor.state().find_error.is_some());
    let mut no_folders = world.editor();
    search(&mut no_folders, "foo");
    no_folders.close_all_folders();
    assert!(!no_folders.find_in_files(false));
    assert_eq!(status(&no_folders), "No open folders or files to search");
    assert!(world.file_search.requests.borrow().is_empty());
}

#[test]
fn a_search_can_be_cancelled() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    search(&mut editor, "foo");
    world.file_search.paused.set(true);
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert!(editor.is_finding_in_files());
    editor.cancel_find_in_files();
    assert!(!editor.is_finding_in_files());
    world.file_search.paused.set(false);
    editor.poll_find_in_files();
    assert_eq!(results(&editor), "Find in Files cancelled\n");
}

#[test]
fn where_snippets_folders_and_find_in_folder() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.add_where_snippet("*.${0:txt}");
    editor.add_where_snippet("<open files>");
    assert_eq!(editor.find_where(), "*.txt,<open files>");
    *world.dialogs.folder_to_open.borrow_mut() = Some(PathBuf::from("/home/user/data/sub"));
    editor.prompt_add_where_folder();
    assert_eq!(
        editor.find_where(),
        "*.txt,<open files>,/home/user/data/sub"
    );
    editor.clear_where();
    assert_eq!(editor.find_where(), "");
    let command = Command::with_args("find_in_folder", json!({ "dirs": ["/a", "/b"] }));
    assert!(editor.side_bar_command_visible(&command));
    assert!(!editor
        .side_bar_command_visible(&Command::with_args("find_in_folder", json!({ "dirs": [] }))));
    editor.find_in_folder(&command);
    assert_eq!(editor.find_where(), "/a,/b,<project filters>");
}

fn tab_text(editor: &Editor) -> String {
    let id = editor
        .find_results_tab()
        .expect("the Find Results tab is open");
    editor
        .state()
        .documents
        .iter()
        .find(|d| d.id == id)
        .unwrap()
        .text
        .clone()
}

#[test]
fn use_buffer_writes_the_results_to_a_tab_with_the_matches_outlined() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    assert!(editor.find_in_files(false));
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.id, tab, "the tab is active");
    assert_eq!(doc.name(), "Find Results");
    assert!(doc.scratch, "closing it does not ask to save");
    assert!(editor.shown_output_panel().is_none(), "no panel shows");
    let text = tab_text(&editor);
    assert!(text.starts_with("Searching 3 files for \"foo\"\n\n"));
    assert!(text.ends_with("4 matches across 2 files\n"));
    let outlined: Vec<String> = doc
        .regions(use_cases::find_in_files::FIND_RESULTS_MATCHES)
        .iter()
        .map(|&(a, b)| text.chars().skip(a).take(b - a).collect())
        .collect();
    assert_eq!(outlined, ["foo", "foo", "foo", "foo"], "one per match");
    let settings = editor.settings_of(tab);
    assert_eq!(settings.str("result_file_regex"), Some(RESULT_FILE_REGEX));
}

#[test]
fn a_second_search_is_added_to_the_same_tab() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    editor.new_file();
    search(&mut editor, "nine");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    assert_eq!(editor.find_results_tab(), Some(tab));
    assert_eq!(editor.active_document().unwrap().id, tab);
    let text = tab_text(&editor);
    assert!(text.contains("4 matches across 2 files\n\nSearching 3 files for \"nine\""));
    assert!(text.ends_with("1 match in 1 file\n"));
}

#[test]
fn next_result_and_a_double_click_go_through_the_tab() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    assert!(editor.next_result(true));
    let doc = editor.active_document().unwrap();
    assert_eq!(
        doc.path.as_deref(),
        Some(Path::new("/home/user/data/a.txt"))
    );
    assert_eq!(doc.selection(), Selection::new(8, 11));
    assert!(editor.next_result(true));
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(44, 47)
    );

    // A double click on `    1: foo` (b.txt), then F4 goes on from there: back to a.txt.
    let text = tab_text(&editor);
    let line = text.find("    1: foo").unwrap();
    let point = text[..line].chars().count() + 3;
    assert!(editor.open_result_in_tab(tab, point));
    assert_eq!(
        editor.active_document().unwrap().path.as_deref(),
        Some(Path::new("/home/user/data/sub/b.txt"))
    );
    assert!(editor.next_result(true));
    assert_eq!(
        editor.active_document().unwrap().path.as_deref(),
        Some(Path::new("/home/user/data/a.txt")),
        "past the last result it starts again"
    );
    // The header is not a result.
    assert!(!editor.open_result_in_tab(tab, 0));
}

#[test]
fn closing_the_tab_ends_its_results() {
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    editor.close_documents(vec![tab]);
    assert_eq!(editor.find_results_tab(), None);
    assert!(!editor.next_result(true));
}

#[test]
fn find_in_files_side_by_side_opens_a_result_beside_the_tab() {
    let world = World::new().with_preferences(json!({ "find_in_files_side_by_side": true }));
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    let text = tab_text(&editor);
    let line = text.find("    1: foo").unwrap();
    let point = text[..line].chars().count() + 3;
    assert!(editor.open_result_in_tab(tab, point));
    assert_eq!(editor.group_count(), 2, "a second column");
    let doc = editor.active_document().unwrap();
    assert_eq!(
        doc.path.as_deref(),
        Some(Path::new("/home/user/data/sub/b.txt"))
    );
    assert_eq!(doc.group, 1, "beside the Find Results tab");
    let results = editor
        .state()
        .documents
        .iter()
        .find(|d| d.id == tab)
        .unwrap();
    assert_eq!(results.group, 0, "which stays where it was");

    // Without the setting, the result opens in the tab's group.
    let world = World::new();
    let mut editor = editor_with_data(&world);
    editor.set_find_use_buffer(true);
    search(&mut editor, "foo");
    editor.find_in_files(false);
    editor.poll_find_in_files();
    let tab = editor.find_results_tab().unwrap();
    assert!(editor.open_result_in_tab(tab, point));
    assert_eq!(editor.group_count(), 1);
}
