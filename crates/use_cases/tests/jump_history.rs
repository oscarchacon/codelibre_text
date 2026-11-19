//! Tests for the window's jump history (`use_cases::jump_history`): Jump Back / Jump Forward and
//! `add_jump_record`.

use std::time::{Duration, Instant};

use entities::selection::Selection;
use serde_json::json;
use test_support::{open, status, World};
use use_cases::Editor;

const TEXT: &str = "one\ntwo\nthree\nfour\nfive\n";

thread_local!(static CLOCK: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) });

/// A frame two seconds after the last one, so nothing merges by time.
fn frame(editor: &mut Editor) {
    let now = CLOCK.with(|clock| {
        let next = clock
            .get()
            .map_or_else(Instant::now, |t| t + Duration::from_secs(2));
        clock.set(Some(next));
        next
    });
    editor.poll_jump_history_at(now);
}

fn two_files() -> (Editor, u64, u64) {
    let world = World::new()
        .with_file("/a.txt", TEXT)
        .with_file("/b.txt", TEXT);
    let mut editor = world.editor();
    open(&mut editor, "/a.txt");
    let a = editor.active_document().unwrap().id;
    frame(&mut editor);
    open(&mut editor, "/b.txt");
    let b = editor.active_document().unwrap().id;
    frame(&mut editor);
    (editor, a, b)
}

/// A click at `point` in the active view, and the frame that follows.
fn click(editor: &mut Editor, point: usize) {
    let id = editor.active_document().unwrap().id;
    editor.note_text_clicked();
    editor.sync_selection_from_view(id, Selection::caret(point));
    frame(editor);
}

fn place(editor: &Editor) -> (u64, Selection) {
    let doc = editor.active_document().unwrap();
    (doc.id, doc.selection())
}

#[test]
fn jump_back_and_forward_go_through_the_places_of_every_view() {
    let (mut editor, a, b) = two_files();
    click(&mut editor, 8);
    editor.activate_document(a);
    frame(&mut editor);
    click(&mut editor, 14);
    editor.note_command("jump_back");
    editor.jump_back();
    frame(&mut editor);
    assert_eq!(place(&editor), (a, Selection::caret(0)));
    editor.note_command("jump_back");
    editor.jump_back();
    frame(&mut editor);
    assert_eq!(
        place(&editor),
        (b, Selection::caret(8)),
        "the other view comes forward"
    );
    editor.note_command("jump_forward");
    editor.jump_forward();
    frame(&mut editor);
    assert_eq!(place(&editor), (a, Selection::caret(0)));
    editor.note_command("jump_forward");
    editor.jump_forward();
    frame(&mut editor);
    assert_eq!(place(&editor), (a, Selection::caret(14)));
    editor.jump_forward();
    assert_eq!(status(&editor), "Already at the newest position");
}

#[test]
fn the_records_move_with_the_text_and_are_not_drawn() {
    let (mut editor, _, b) = two_files();
    click(&mut editor, 14);
    click(&mut editor, 4);
    let id = editor.active_document().unwrap().id;
    assert_eq!(id, b);
    editor.sync_selection_from_view(id, Selection::caret(0));
    editor.insert_at_selections("zero\n");
    frame(&mut editor);
    let records = editor.jump_records();
    let places: Vec<_> = records.iter().map(|(_, s)| s.clone()).collect();
    assert!(
        places.contains(&vec![(19, 19)]),
        "14 moved after the insert: {places:?}"
    );
    let doc = editor.active_document().unwrap();
    assert!(doc
        .region_sets()
        .iter()
        .filter(|set| set.key.starts_with("jump_key_"))
        .all(|set| set.flags & 128 != 0));
}

#[test]
fn a_plugin_s_record_keeps_the_selection_where_it_is() {
    let (mut editor, _, b) = two_files();
    editor.add_jump_record(Some(&json!([[2, 5], 9]))).unwrap();
    assert_eq!(
        editor.jump_records().last(),
        Some(&(b, vec![(2, 5), (9, 9)]))
    );
    assert_eq!(place(&editor), (b, Selection::caret(0)));
    editor.add_jump_record(Some(&json!(4))).unwrap();
    assert_eq!(editor.jump_records().last(), Some(&(b, vec![(4, 4)])));
    editor.add_jump_record(Some(&json!([1, 3]))).unwrap();
    assert_eq!(editor.jump_records().last(), Some(&(b, vec![(1, 3)])));
    assert!(editor.add_jump_record(Some(&json!("x"))).is_err());
    assert!(editor.add_jump_record(None).is_err());
    editor.jump_back();
    assert_eq!(place(&editor), (b, Selection::caret(4)));
}

#[test]
fn closing_a_view_forgets_its_places() {
    let (mut editor, a, b) = two_files();
    click(&mut editor, 8);
    editor.close_active_document();
    frame(&mut editor);
    assert!(editor.jump_records().iter().all(|(view, _)| *view == a));
    assert!(editor.jump_records().iter().all(|(view, _)| *view != b));
    editor.jump_back();
    assert_eq!(status(&editor), "Already at the earliest position");
}

#[test]
fn each_window_has_its_own_history() {
    let (mut editor, _, _) = two_files();
    click(&mut editor, 8);
    let first = editor.current_window();
    let before = editor.jump_records().len();
    editor.new_window();
    frame(&mut editor);
    assert!(editor.jump_records().len() <= 1);
    editor.switch_to_window(first);
    assert_eq!(editor.jump_records().len(), before);
}
