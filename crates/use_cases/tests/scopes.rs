//! Tests for the scope use cases (`use_cases::scopes`). The fake syntax gateway gives every line
//! the syntax's base scope, and text between double quotes `string.quoted.double` too.

use std::path::Path;

use entities::selection::Selection;
use test_support::World;
use use_cases::scopes::ScopedRange;
use use_cases::Editor;

const CODE: &str = "x = \"hi\"\ny = 1\nz = \"a\"";

fn python(world: &World) -> Editor {
    world.disk.put("/p/a.py", CODE);
    let mut editor = world.editor();
    editor.open_file(Path::new("/p/a.py"));
    editor
}

fn id(editor: &Editor) -> u64 {
    editor.active_document().unwrap().id
}

#[test]
fn scope_name_is_the_scope_of_the_char_at_the_point() {
    let world = World::new();
    let editor = python(&world);
    assert_eq!(editor.scope_name(id(&editor), 0).unwrap(), "source.python ");
    assert_eq!(editor.scope_name(id(&editor), 4).unwrap(), "source.python string.quoted.double ", "the quote");
    assert_eq!(editor.scope_name(id(&editor), 7).unwrap(), "source.python string.quoted.double ");
    assert_eq!(editor.scope_name(id(&editor), 8).unwrap(), "source.python ", "the newline after it");
    let end = CODE.chars().count();
    assert_eq!(editor.scope_name(id(&editor), end).unwrap(), "source.python string.quoted.double ", "the last char's");
    assert_eq!(editor.scope_name(999, 0), None);
}

#[test]
fn an_empty_document_has_its_syntax_base_scope() {
    let world = World::new();
    let editor = world.editor();
    assert_eq!(editor.scope_name(id(&editor), 0).unwrap(), "text.plain ");
}

#[test]
fn find_by_selector_joins_neighbouring_pieces() {
    let world = World::new();
    let editor = python(&world);
    assert_eq!(editor.find_by_selector(id(&editor), "string"), vec![(4, 8), (19, 22)]);
    assert_eq!(editor.find_by_selector(id(&editor), "source - string"), vec![(0, 4), (8, 19)]);
}

#[test]
fn extract_scope_spans_the_token_around_the_point() {
    let world = World::new();
    let editor = python(&world);
    assert_eq!(editor.extract_scope(id(&editor), 5), Some((4, 8)));
}

#[test]
fn scoped_ranges_are_clipped_to_the_region() {
    let world = World::new();
    let editor = python(&world);
    let scope = |s: &str| s.to_string();
    assert_eq!(
        editor.scoped_ranges(id(&editor), 2, 6),
        vec![
            ScopedRange { start: 2, end: 4, scope: scope("source.python") },
            ScopedRange { start: 4, end: 6, scope: scope("source.python string.quoted.double") },
        ]
    );
}

#[test]
fn caret_scopes_has_one_scope_per_selection() {
    let world = World::new();
    let mut editor = python(&world);
    editor.sync_selection_from_view(id(&editor), Selection::caret(5));
    assert_eq!(editor.caret_scopes(), vec!["source.python string.quoted.double "]);
    // Alt+F3 on the first `=`: one caret per `=`.
    editor.sync_selection_from_view(id(&editor), Selection::new(2, 3));
    editor.select_all_occurrences();
    assert_eq!(editor.caret_scopes(), vec!["source.python "; 3]);
}
