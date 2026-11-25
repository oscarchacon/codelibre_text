//! Tests for the find / replace use cases (`use_cases::find_replace`).

use entities::find::FindOptions;
use entities::selection::Selection;
use test_support::{active_text, status, World};
use use_cases::editing::{DragBy, DragCombine, DragPoint, DragSelect, TextCommand};
use use_cases::Editor;

fn editor_with(world: &World, text: &str, selection: Selection) -> Editor {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, selection);
    editor
}

fn selection(editor: &Editor) -> Selection {
    editor.active_document().unwrap().selection()
}

fn query(text: &str) -> FindOptions {
    FindOptions {
        query: text.into(),
        ..Default::default()
    }
}

#[test]
fn a_single_line_selection_becomes_the_query_when_the_panel_opens() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat dog", Selection::new(4, 7));
    editor.use_selection_as_query_if_single_line();
    assert_eq!(editor.state().find.query, "dog");
}

#[test]
fn multi_line_selections_do_not_replace_the_query() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nb", Selection::new(0, 3));
    editor.set_find_options(query("keep"));
    editor.use_selection_as_query_if_single_line();
    assert_eq!(editor.state().find.query, "keep");
}

#[test]
fn find_under_caret_selects_the_word_and_searches_it_as_a_whole_word() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo bar", Selection::caret(5));
    editor.set_find_options(FindOptions {
        regex: true,
        ..query("x")
    });
    editor.use_selection_as_query(true);
    let find = &editor.state().find;
    assert_eq!(find.query, "bar");
    assert!(find.whole_word && !find.regex);
    assert_eq!(selection(&editor), Selection::new(4, 7));
}

#[test]
fn using_an_explicit_selection_is_not_whole_word() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo bar", Selection::new(0, 2));
    editor.use_selection_as_query(true);
    assert_eq!(editor.state().find.query, "fo");
    assert!(!editor.state().find.whole_word);
}

#[test]
fn find_next_and_previous_move_the_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat dog cat", Selection::caret(0));
    editor.set_find_options(query("cat"));
    assert!(editor.find_next(true));
    assert_eq!(selection(&editor), Selection::new(0, 3));
    editor.find_next(true);
    assert_eq!(selection(&editor), Selection::new(8, 11));
    editor.find_next(false);
    assert_eq!(selection(&editor), Selection::new(0, 3));
}

#[test]
fn find_next_with_an_empty_query_reports_nothing_to_search() {
    let world = World::new();
    let mut editor = world.editor();
    assert!(!editor.find_next(true));
}

#[test]
fn not_found_is_reported_in_the_status_bar() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(query("zzz"));
    editor.find_next(true);
    assert_eq!(status(&editor), "Unable to find zzz");
}

#[test]
fn an_invalid_regex_is_shown_as_a_find_error() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(FindOptions {
        regex: true,
        ..query("(")
    });
    assert!(editor.find_next(true));
    assert!(editor.state().find_error.is_some());
}

#[test]
fn ctrl_d_selects_the_word_then_adds_the_next_occurrences() {
    let world = World::new();
    let mut editor = editor_with(&world, "x = x + x", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(0, 1));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(4, 5));
    editor.select_word_or_next_occurrence();
    let doc = editor.active_document().unwrap();
    assert_eq!(
        doc.selections(),
        vec![
            Selection::new(0, 1),
            Selection::new(4, 5),
            Selection::new(8, 9)
        ]
    );
    editor.select_word_or_next_occurrence();
    assert_eq!(
        editor.active_document().unwrap().selections().len(),
        3,
        "every occurrence is already selected"
    );
}

/// Selections as (anchor, head) pairs.
type Ranges = &'static [(usize, usize)];

/// A text, its selections, the steps run (`d`, `k`) and the selections after each step.
type StepsCase = (&'static str, Ranges, &'static str, &'static [Ranges]);

/// `text` with `selections` added in this order (the first one as a plain selection, the others
/// as Ctrl + clicks / drags would add them), then `steps` run: `d` is Ctrl+D, `k` Ctrl+K Ctrl+D.
/// Returns the selections (anchor, head) after each step.
fn run_steps(text: &str, selections: &[(usize, usize)], steps: &str) -> Vec<Vec<(usize, usize)>> {
    let world = World::new();
    let (a, b) = selections[0];
    let mut editor = editor_with(&world, text, Selection::new(a, b));
    let id = editor.active_document().unwrap().id;
    for &(a, b) in &selections[1..] {
        let drag = DragSelect {
            anchor: DragPoint::at(a),
            head: DragPoint::at(b),
            by: DragBy::Characters,
            combine: DragCombine::Add,
            pressed: true,
        };
        editor.drag_select(id, drag);
        editor.end_drag_select();
    }
    steps
        .chars()
        .map(|step| {
            match step {
                'd' => editor.select_word_or_next_occurrence(),
                'k' => editor.skip_to_next_occurrence(),
                other => panic!("unknown step {other}"),
            }
            let doc = editor.active_document().unwrap();
            doc.selections()
                .iter()
                .map(|s| (s.anchor, s.head))
                .collect()
        })
        .collect()
}

/// Ctrl+D and Ctrl+K Ctrl+D as Sublime 4215 answered them (a plugin running
/// `find_under_expand` / `find_under_expand_skip` on scratch views, 2026-11-24).
#[test]
fn ctrl_d_and_ctrl_k_ctrl_d_do_what_sublime_does() {
    let cases: &[StepsCase] = &[
        // A caret: its word, then skipped for the next one; skips go on from the last selection.
        (
            "x = x + x + x",
            &[(0, 0)],
            "kkdkkk",
            &[
                &[(4, 5)],
                &[(8, 9)],
                &[(8, 9), (12, 13)],
                &[(0, 1), (8, 9)],
                &[(0, 1), (12, 13)],
                &[(0, 1), (4, 5)],
            ],
        ),
        // Every occurrence selected: the last one is only taken away...
        ("ab ab", &[(0, 2)], "dk", &[&[(0, 2), (3, 5)], &[(0, 2)]]),
        // ...unless it is the only selection.
        ("ab cd", &[(0, 2)], "k", &[&[(0, 2)]]),
        // The last by position, not the newest: Ctrl+D wrapped to 0, Ctrl+K Ctrl+D takes 12 away.
        (
            "x = x + x + x",
            &[(8, 9)],
            "ddkk",
            &[
                &[(8, 9), (12, 13)],
                &[(0, 1), (8, 9), (12, 13)],
                &[(0, 1), (4, 5), (8, 9)],
                &[(0, 1), (4, 5), (12, 13)],
            ],
        ),
        (
            "x = x + x + x",
            &[(8, 9), (0, 1)],
            "d",
            &[&[(0, 1), (8, 9), (12, 13)]],
        ),
        (
            "x = x + x + x",
            &[(8, 9), (0, 1)],
            "kd",
            &[&[(0, 1), (12, 13)], &[(0, 1), (4, 5), (12, 13)]],
        ),
        // The last selection's text is looked for, not the primary one's.
        (
            "ab cd ab cd",
            &[(3, 5), (0, 2)],
            "d",
            &[&[(0, 2), (3, 5), (9, 11)]],
        ),
        ("ab cd ab cd", &[(3, 5), (0, 2)], "k", &[&[(0, 2), (9, 11)]]),
        // A reversed selection; the occurrence found is forward.
        (
            "ab ab ab",
            &[(2, 0)],
            "kdk",
            &[&[(3, 5)], &[(3, 5), (6, 8)], &[(0, 2), (3, 5)]],
        ),
        ("ab ab ab", &[(0, 2), (5, 3)], "k", &[&[(0, 2), (6, 8)]]),
        // Part of a word matches inside words; a caret's word only whole words.
        ("foo foobar foo", &[(0, 2)], "kk", &[&[(4, 6)], &[(11, 13)]]),
        ("ab abc ab", &[(1, 1)], "k", &[&[(7, 9)]]),
        // Carets select their words first: Ctrl+D adds nothing then, Ctrl+K Ctrl+D goes on.
        (
            "ab cd ab",
            &[(1, 1), (4, 4)],
            "dd",
            &[&[(0, 2), (3, 5)], &[(0, 2), (3, 5)]],
        ),
        (
            "ab cd ab cd",
            &[(0, 2), (4, 4)],
            "dd",
            &[&[(0, 2), (3, 5)], &[(0, 2), (3, 5), (9, 11)]],
        ),
        ("ab cd ab cd", &[(1, 1), (3, 5)], "d", &[&[(0, 2), (3, 5)]]),
        ("ab cd ab cd", &[(1, 1), (4, 4)], "k", &[&[(0, 2), (9, 11)]]),
        ("ab cd ab cd", &[(0, 2), (4, 4)], "k", &[&[(0, 2), (9, 11)]]),
        ("ab ab ab", &[(0, 2), (4, 4)], "k", &[&[(0, 2), (6, 8)]]),
        ("ab cd ab cd", &[(1, 1), (6, 8)], "k", &[&[(0, 2)]]),
        (
            "ab cd ab cd ab",
            &[(1, 1), (6, 8)],
            "k",
            &[&[(0, 2), (12, 14)]],
        ),
        // A caret between words selects the blanks around it; at the end, the word before.
        ("a  b  c", &[(2, 2)], "dd", &[&[(1, 3)], &[(1, 3), (4, 6)]]),
        ("a  b", &[(2, 2)], "k", &[&[(1, 3)]]),
        ("ab ab", &[(5, 5)], "d", &[&[(3, 5)]]),
        ("a.b a.b", &[(1, 1)], "d", &[&[(0, 1)]]),
        // A caret off words takes Sublime's `view.word`, then looks for whole words: `\b`
        // around punctuation finds nothing, around blanks between words it does.
        ("x = x", &[(2, 2)], "d", &[&[(1, 3)]]),
        ("a += b += c", &[(3, 3)], "dd", &[&[(2, 4)], &[(2, 4)]]),
        ("x =  y =  z", &[(3, 3)], "dd", &[&[(2, 5)], &[(2, 5)]]),
        (
            "a    b    c",
            &[(3, 3)],
            "dd",
            &[&[(1, 5)], &[(1, 5), (6, 10)]],
        ),
        (
            "a...b...c",
            &[(2, 2)],
            "dd",
            &[&[(1, 4)], &[(1, 4), (5, 8)]],
        ),
        ("a, b, c", &[(2, 2)], "dd", &[&[(1, 3)], &[(1, 3), (4, 6)]]),
        (
            "ab \ncd \nef",
            &[(3, 3)],
            "dd",
            &[&[(2, 4)], &[(2, 4), (6, 8)]],
        ),
        ("a\n\nb", &[(2, 2)], "d", &[&[(2, 2)]]),
        ("a-b-c", &[(1, 1)], "dd", &[&[(0, 1)], &[(0, 1)]]),
        ("x = x = x", &[(2, 2)], "kk", &[&[(1, 3)], &[(1, 3)]]),
        (
            "f(a, b) f(a, b)",
            &[(1, 1)],
            "dd",
            &[&[(0, 1)], &[(0, 1), (8, 9)]],
        ),
        // A selection made by hand matches inside words.
        (
            "ab abc ab",
            &[(0, 2)],
            "dd",
            &[&[(0, 2), (3, 5)], &[(0, 2), (3, 5), (7, 9)]],
        ),
        ("a += b += c", &[(2, 4)], "d", &[&[(2, 4), (7, 9)]]),
        ("x + y + z", &[(1, 4)], "d", &[&[(1, 4), (5, 8)]]),
        ("ab-ab-ab", &[(2, 3)], "d", &[&[(2, 3), (5, 6)]]),
    ];
    for (text, selections, steps, expected) in cases {
        let expected: Vec<Vec<(usize, usize)>> = expected.iter().map(|s| s.to_vec()).collect();
        assert_eq!(
            run_steps(text, selections, steps),
            expected,
            "{text:?} from {selections:?}, steps {steps}"
        );
    }
}

#[test]
fn the_selection_ctrl_k_ctrl_d_finds_is_the_primary_one() {
    let world = World::new();
    let mut editor = editor_with(&world, "x = x + x", Selection::new(0, 1));
    editor.skip_to_next_occurrence();
    assert_eq!(
        selection(&editor),
        Selection::new(4, 5),
        "the view follows it"
    );
}

#[test]
fn ctrl_d_from_a_word_matches_whole_words_and_case() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo Foo foobar foo", Selection::caret(1));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(15, 18));
}

#[test]
fn ctrl_d_from_a_selection_matches_inside_words() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo foobar", Selection::new(0, 3));
    editor.select_word_or_next_occurrence();
    assert_eq!(
        selection(&editor),
        Selection::new(4, 7),
        "a selection made by hand is not a whole word, even when it is one (Sublime 4215)"
    );
    let mut editor = editor_with(&world, "fo foobar", Selection::new(0, 1));
    editor.select_word_or_next_occurrence();
    assert_eq!(
        selection(&editor),
        Selection::new(3, 4),
        "part of a word also matches inside words"
    );
}

#[test]
fn typing_deleting_and_moving_with_several_selections() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab ab", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    editor.insert_at_selections("xyz");
    assert_eq!(active_text(&editor), "xyz xyz");
    editor.delete_at_selections(false);
    assert_eq!(active_text(&editor), "xy xy");
    editor.move_carets(entities::text_commands::CaretMotion::LineStart);
    editor.delete_at_selections(true);
    assert_eq!(
        active_text(&editor),
        "y xy",
        "both carets went to the line start and merged"
    );
    assert!(editor
        .active_document()
        .unwrap()
        .extra_selections()
        .is_empty());
}

#[test]
fn escape_keeps_only_the_primary_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "a a", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    editor.single_selection();
    let doc = editor.active_document().unwrap();
    assert!(doc.extra_selections().is_empty());
    assert_eq!(doc.selection(), Selection::new(2, 3));
}

#[test]
fn replace_next_replaces_the_selected_match() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat cat", Selection::new(0, 3));
    editor.set_find_options(query("cat"));
    editor.set_replacement("dog".into());
    editor.replace_next();
    assert_eq!(active_text(&editor), "dog cat");
    assert_eq!(selection(&editor), Selection::new(4, 7));
}

#[test]
fn replace_all_reports_how_many_were_replaced() {
    let world = World::new();
    let mut editor = editor_with(&world, "a-a-a", Selection::caret(0));
    editor.set_find_options(query("a"));
    editor.set_replacement("b".into());
    editor.replace_all();
    assert_eq!(active_text(&editor), "b-b-b");
    assert_eq!(status(&editor), "Replaced 3 occurrences");
}

#[test]
fn replace_with_an_empty_query_does_nothing() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.replace_all();
    editor.replace_next();
    assert_eq!(active_text(&editor), "abc");
}

#[test]
fn find_matches_lists_every_match_and_follows_edits() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab ab", Selection::caret(0));
    editor.set_find_options(query("ab"));
    assert_eq!(editor.find_matches(), vec![0..2, 3..5]);
    editor.run_text_command(TextCommand::Insert("ab ".into()));
    assert_eq!(editor.find_matches().len(), 3);
}

#[test]
fn find_matches_records_regex_errors() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(FindOptions {
        regex: true,
        ..query("[")
    });
    assert!(editor.find_matches().is_empty());
    assert!(editor.state().find_error.is_some());
    editor.set_find_options(query("b"));
    assert_eq!(editor.find_matches(), vec![1..2]);
    assert!(editor.state().find_error.is_none());
}

#[test]
fn find_matches_is_empty_without_a_query() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    assert!(editor.find_matches().is_empty());
}

#[test]
fn find_all_under_selects_every_whole_word_occurrence_of_the_word_at_the_caret() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab abc ab Ab", Selection::caret(8));
    editor.select_all_occurrences();
    let doc = editor.active_document().unwrap();
    assert_eq!(
        doc.selection(),
        Selection::new(7, 9),
        "the primary selection stays where the caret was"
    );
    assert_eq!(
        doc.selections(),
        vec![Selection::new(0, 2), Selection::new(7, 9)],
        "case sensitive, whole words"
    );
}

#[test]
fn find_all_under_with_a_partial_selection_matches_inside_words() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab abc", Selection::new(0, 1));
    editor.select_all_occurrences();
    assert_eq!(
        editor.active_document().unwrap().selections(),
        vec![Selection::new(0, 1), Selection::new(3, 4)]
    );
}

#[test]
fn find_all_under_without_a_word_does_nothing() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\n\nb", Selection::caret(2));
    editor.select_all_occurrences();
    assert_eq!(selection(&editor), Selection::caret(2), "an empty line");
}

/// Alt+F3 as Sublime 4215 answered it (2026-11-24): a selection made by hand matches inside
/// words; a caret's word only whole words, so `+=` under a caret finds nothing.
#[test]
fn find_all_under_does_what_sublime_does() {
    let cases: &[(&str, Selection, Ranges)] = &[
        ("ab abc ab", Selection::new(0, 2), &[(0, 2), (3, 5), (7, 9)]),
        ("ab abc ab", Selection::caret(1), &[(0, 2), (7, 9)]),
        ("a += b += c", Selection::caret(3), &[(3, 3)]),
        ("a += b += c", Selection::new(2, 4), &[(2, 4), (7, 9)]),
    ];
    let world = World::new();
    for &(text, start, expected) in cases {
        let mut editor = editor_with(&world, text, start);
        editor.select_all_occurrences();
        let got: Vec<(usize, usize)> = editor
            .active_document()
            .unwrap()
            .selections()
            .iter()
            .map(|s| (s.anchor, s.head))
            .collect();
        assert_eq!(got, expected, "{text:?} from {start:?}");
    }
}

/// Ctrl+D from carets makes the view's occurrences whole words (until a command changes the
/// view: `command_controller`'s tests); `copy` and `insert` end them at once, as Sublime 4215
/// always records them.
#[test]
fn whole_word_occurrences_end_with_copy_or_insert() {
    let world = World::new();
    let ctrl_d_on_plus_equals = |between: &[&str]| {
        let mut editor = editor_with(&world, "a += b += c", Selection::caret(3));
        editor.note_command("find_under_expand");
        editor.select_word_or_next_occurrence();
        assert_eq!(selection(&editor), Selection::new(2, 4));
        for name in between {
            editor.note_command(name);
        }
        editor.note_command("find_under_expand");
        editor.select_word_or_next_occurrence();
        editor.active_document().unwrap().selections().len()
    };
    assert_eq!(ctrl_d_on_plus_equals(&[]), 1, "still whole words: no match");
    assert_eq!(ctrl_d_on_plus_equals(&["show_overlay"]), 1);
    assert_eq!(ctrl_d_on_plus_equals(&["copy"]), 2, "a plain search again");
    assert_eq!(ctrl_d_on_plus_equals(&["insert"]), 2);
}
