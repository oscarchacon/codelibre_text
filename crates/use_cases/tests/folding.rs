//! Tests for code folding (`use_cases::folding`).

use entities::selection::Selection;
use test_support::World;
use use_cases::Editor;

const CODE: &str = "fn a() {\n    one();\n}\nfn b() {\n    two();\n}\n";

fn editor_with_code(world: &World) -> (Editor, u64) {
    let mut editor = world.editor();
    editor.insert_at_selections(CODE);
    let id = editor.active_document().unwrap().id;
    (editor, id)
}

fn folds(editor: &Editor) -> Vec<(usize, usize)> {
    editor.active_document().unwrap().folds().to_vec()
}

#[test]
fn fold_folds_the_block_at_the_caret_and_moves_the_caret_out() {
    let world = World::new();
    let (mut editor, id) = editor_with_code(&world);
    editor.sync_selection_from_view(id, Selection::caret(12));
    editor.fold_selections();
    assert_eq!(folds(&editor), vec![(8, 19)]);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(8)
    );
    // A selection folds itself.
    editor.sync_selection_from_view(id, Selection::new(21, 24));
    editor.fold_selections();
    assert_eq!(folds(&editor), vec![(8, 19), (21, 24)]);
    editor.unfold_all();
    assert!(folds(&editor).is_empty());
}

#[test]
fn unfold_opens_the_folds_on_the_caret_s_line_and_a_caret_inside_opens_one() {
    let world = World::new();
    let (mut editor, id) = editor_with_code(&world);
    editor.fold_by_level(1);
    assert_eq!(folds(&editor), vec![(8, 19), (30, 41)]);
    editor.sync_selection_from_view(id, Selection::caret(2));
    editor.unfold_selections();
    assert_eq!(folds(&editor), vec![(30, 41)]);
    // A click (or a key) puts the caret inside the other: it opens.
    editor.sync_selection_from_view(id, Selection::caret(35));
    assert!(folds(&editor).is_empty());
    // The view's marker.
    editor.fold_by_level(1);
    editor.unfold_at(id, 30);
    assert_eq!(folds(&editor), vec![(8, 19)]);
}

#[test]
fn a_gutter_button_folds_the_block_its_line_heads_and_opens_it_again() {
    let world = World::new();
    let (mut editor, id) = editor_with_code(&world);
    editor.sync_selection_from_view(id, Selection::caret(35));
    editor.toggle_fold_at_line(id, 3);
    assert_eq!(folds(&editor), vec![(30, 41)]);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(30),
        "the caret inside went before the fold"
    );
    editor.toggle_fold_at_line(id, 0);
    assert_eq!(folds(&editor), vec![(8, 19), (30, 41)]);
    editor.toggle_fold_at_line(id, 3);
    assert_eq!(folds(&editor), vec![(8, 19)]);
    // A line past the end, or one that heads nothing, does nothing.
    editor.toggle_fold_at_line(id, 99);
    editor.toggle_fold_at_line(id, 2);
    assert_eq!(folds(&editor), vec![(8, 19)]);
}
