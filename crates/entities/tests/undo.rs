//! Tests for the undo history (`entities::undo`) as a `Document` keeps it: what is one step,
//! undoing and redoing, the selections they put back.

use entities::document::{Document, RegionSet};
use entities::selection::Selection;

fn document(text: &str) -> Document {
    let mut doc = Document::scratch(1, "t", text, "Plain Text", false);
    doc.set_selection(Selection::caret(text.chars().count()));
    doc
}

/// What the text widget does: edits the text in place, then the view reports the change and
/// the caret.
fn type_at_caret(doc: &mut Document, typed: &str) {
    let at = doc.selection().head;
    let byte = entities::text::char_to_byte(&doc.text, at);
    doc.text_for_widget().insert_str(byte, typed);
    doc.note_typed_change();
    doc.sync_selection_from_view(Selection::caret(at + typed.chars().count()));
}

fn backspace(doc: &mut Document) {
    let at = doc.selection().head;
    let (from, to) = (
        entities::text::char_to_byte(&doc.text, at - 1),
        entities::text::char_to_byte(&doc.text, at),
    );
    doc.text_for_widget().replace_range(from..to, "");
    doc.note_typed_change();
    doc.sync_selection_from_view(Selection::caret(at - 1));
}

#[test]
fn typing_in_a_row_is_one_step_and_undo_puts_the_caret_back() {
    let mut doc = document("x ");
    for c in ["h", "e", "l", "l", "o"] {
        type_at_caret(&mut doc, c);
    }
    assert_eq!(doc.text, "x hello");
    assert_eq!(doc.undo_history().undo_count(), 1);
    assert!(doc.undo());
    assert_eq!(doc.text, "x ");
    assert_eq!(doc.selection(), Selection::caret(2));
    assert!(doc.redo());
    assert_eq!(doc.text, "x hello");
    assert_eq!(doc.selection(), Selection::caret(7));
    assert!(!doc.redo(), "nothing more to redo");
}

#[test]
fn backspaces_join_the_typing_they_follow() {
    let mut doc = document("");
    type_at_caret(&mut doc, "a");
    type_at_caret(&mut doc, "b");
    type_at_caret(&mut doc, "c");
    backspace(&mut doc);
    type_at_caret(&mut doc, "d");
    assert_eq!(doc.text, "abd");
    assert_eq!(doc.undo_history().undo_count(), 1);
    assert!(doc.undo());
    assert_eq!(doc.text, "");
}

#[test]
fn a_line_break_a_paste_and_a_caret_moved_away_start_new_steps() {
    let mut doc = document("");
    type_at_caret(&mut doc, "a");
    type_at_caret(&mut doc, "\n");
    type_at_caret(&mut doc, "b");
    type_at_caret(&mut doc, "pasted");
    type_at_caret(&mut doc, "c");
    // A click at the start, then typing there.
    doc.sync_selection_from_view(Selection::caret(0));
    type_at_caret(&mut doc, "z");
    assert_eq!(doc.text, "za\nbpastedc");
    let mut texts = Vec::new();
    while doc.undo() {
        texts.push(doc.text.clone());
    }
    assert_eq!(
        texts,
        ["a\nbpastedc", "a\nbpasted", "a\nb", "a", ""],
        "the click, the char after the paste, the paste, the new line, the first line"
    );
}

#[test]
fn a_command_s_edits_are_one_step_until_it_is_closed() {
    let mut doc = document("abc");
    doc.close_undo_step();
    doc.replace_range(0, 0, "1");
    doc.replace_range(4, 4, "2");
    doc.close_command_undo_step();
    doc.replace_range(0, 1, "");
    assert_eq!(doc.text, "abc2");
    assert!(doc.undo());
    assert_eq!(doc.text, "1abc2");
    assert!(doc.undo());
    assert_eq!(doc.text, "abc", "both edits of the first command");
    assert!(!doc.undo(), "the scratch text is where the history starts");
}

#[test]
fn a_new_edit_forgets_what_was_undone() {
    let mut doc = document("");
    type_at_caret(&mut doc, "a");
    assert!(doc.undo());
    type_at_caret(&mut doc, "b");
    assert!(!doc.redo());
    assert_eq!(doc.text, "b");
}

#[test]
fn undo_moves_regions_and_skips_read_only_documents() {
    let mut doc = document("one two");
    doc.add_regions(RegionSet {
        key: "k".into(),
        regions: vec![(4, 7)],
        ..RegionSet::default()
    });
    doc.close_undo_step();
    doc.replace_range(0, 0, ">> ");
    assert_eq!(doc.regions("k"), [(7, 10)]);
    assert!(doc.undo());
    assert_eq!(doc.regions("k"), [(4, 7)], "the region follows the undo");

    doc.read_only = true;
    assert!(!doc.redo());
    assert_eq!(doc.text, "one two");
}

#[test]
fn clones_take_the_buffer_s_history() {
    let mut doc = document("");
    type_at_caret(&mut doc, "a");
    let mut clone = doc.clone_view(2);
    type_at_caret(&mut doc, "\n");
    clone.follow_buffer(&doc);
    assert_eq!(clone.text, "a\n");
    assert!(clone.undo(), "the clone undoes the edit made in the other view");
    assert_eq!(clone.text, "a");
    doc.follow_buffer(&clone);
    assert!(doc.redo(), "and the other view can redo it");
    assert_eq!(doc.text, "a\n");
}
