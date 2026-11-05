//! Tests for auto-complete (`use_cases::completions`).

use entities::command::Command;
use entities::selection::Selection;
use serde_json::json;
use test_support::{active_text, plugin_call, World, PACKAGES};
use use_cases::completions::AutoCompleteOptions;
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
use use_cases::Editor;

/// An editor with `path` open, `text` typed in it as if by the user.
fn typed(world: &World, path: &str, text: &str) -> Editor {
    let mut editor = world.editor_opening(&[path]);
    let doc = editor.active_document().unwrap();
    let (id, end) = (doc.id, doc.char_count());
    editor.sync_selection_from_view(id, Selection::caret(end));
    type_text(&mut editor, text);
    editor
}

fn type_text(editor: &mut Editor, text: &str) {
    let id = editor.active_document().unwrap().id;
    editor.insert_at_selections(text);
    editor.typed_in_view(id, text);
    editor.poll_plugins(&mut |_, _| {});
}

fn triggers(editor: &Editor) -> Vec<String> {
    editor.auto_complete().map(|l| l.shown.iter().map(|s| s.completion.trigger.clone()).collect()).unwrap_or_default()
}

fn caret(editor: &Editor) -> usize {
    editor.active_document().unwrap().selection().head
}

#[test]
fn typing_a_word_in_source_offers_the_files_words_and_filters_them() {
    let world = World::new().with_file("/p/a.py", "alphabet beta\nalpine\n");
    let mut editor = typed(&world, "/p/a.py", "al");
    let shown = triggers(&editor);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert!(shown.contains(&"alphabet".into()) && shown.contains(&"alpine".into()));

    type_text(&mut editor, "pi");
    assert_eq!(triggers(&editor), vec!["alpine"]);

    // Leaving the word closes it.
    type_text(&mut editor, " ");
    assert!(!editor.auto_complete_visible());
}

#[test]
fn plain_text_only_shows_it_on_request_and_esc_or_another_view_hide_it() {
    let world = World::new().with_file("/p/notes.txt", "hello help\n").with_file("/p/b.txt", "");
    let mut editor = typed(&world, "/p/notes.txt", "he");
    assert!(!editor.auto_complete_visible(), "text.plain is not in auto_complete_selector");

    editor.show_auto_complete(AutoCompleteOptions::default());
    assert_eq!(triggers(&editor).len(), 2);
    editor.hide_auto_complete();
    assert!(!editor.auto_complete_visible());

    editor.show_auto_complete(AutoCompleteOptions::default());
    editor.open_file(std::path::Path::new("/p/b.txt"));
    editor.poll_plugins(&mut |_, _| {});
    assert!(!editor.auto_complete_visible());
}

#[test]
fn committing_replaces_the_typed_word() {
    let world = World::new().with_file("/p/a.py", "configuration\n");
    let mut editor = typed(&world, "/p/a.py", "x = conf");
    assert_eq!(triggers(&editor), vec!["configuration"]);
    editor.commit_completion(None);
    assert_eq!(active_text(&editor), "configuration\nx = configuration");
    assert_eq!(caret(&editor), "configuration\nx = configuration".len());
    assert!(!editor.auto_complete_visible());
}

#[test]
fn the_selection_moves_and_wraps_around() {
    let world = World::new().with_file("/p/a.py", "ab1 ab2 ab3\n");
    let mut editor = typed(&world, "/p/a.py", "\nab");
    assert_eq!(triggers(&editor).len(), 3);
    editor.move_auto_complete_selection(-1);
    assert_eq!(editor.auto_complete().unwrap().selected, 2);
    editor.move_auto_complete_selection(1);
    assert_eq!(editor.auto_complete().unwrap().selected, 0);
    editor.move_auto_complete_selection(8);
    assert_eq!(editor.auto_complete().unwrap().selected, 2, "a page stops at the end");
}

#[test]
fn packages_completions_and_snippets_apply_where_their_scope_matches() {
    let snippet = "<snippet><content><![CDATA[for ${1:x} in ${2:y}:]]></content><tabTrigger>for</tabTrigger><scope>source.python</scope></snippet>";
    let world = World::new()
        .with_file("/p/a.py", "")
        .with_file("/p/b.rs", "")
        .with_resource("Packages/Py/for.sublime-snippet", snippet)
        .with_resource("Packages/Py/py.sublime-completions", r#"{"scope": "source.python", "completions": ["format"]}"#);
    let mut editor = typed(&world, "/p/a.py", "fo");
    let shown = triggers(&editor);
    assert_eq!(shown.len(), 2, "{shown:?}");
    editor.commit_completion(shown.iter().position(|t| t == "for"));
    assert_eq!(active_text(&editor), "for x in y:");
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::new(4, 5), "the first field is selected");

    let mut rust = typed(&world, "/p/b.rs", "fo");
    rust.show_auto_complete(AutoCompleteOptions::default());
    assert!(!rust.auto_complete_visible(), "nothing for source.rs");
}

#[test]
fn tab_expands_a_trigger_typed_in_full() {
    let snippet = "<snippet><content><![CDATA[def ${1:name}():]]></content><tabTrigger>def</tabTrigger></snippet>";
    let world = World::new().with_file("/p/a.py", "").with_resource("Packages/Py/def.sublime-snippet", snippet);
    let mut editor = world.editor_opening(&["/p/a.py"]);
    editor.insert_at_selections("de");
    assert!(!editor.tab_completion(), "not the whole trigger");
    editor.insert_at_selections("f");
    assert!(editor.tab_completion());
    assert_eq!(active_text(&editor), "def name():");
}

/// An editor whose plugins listen to `on_query_completions`.
fn plugin_world() -> World {
    let world = World::new().with_file("/p/a.py", "plugh\n").with_file(&format!("{PACKAGES}/P/p.py"), "");
    world.plugins.then([PluginMessage::Loaded { commands: vec![], events: vec!["on_query_completions".into()], errors: vec![] }]);
    world
}

#[test]
fn plugins_completions_come_first_and_can_leave_out_the_words() {
    let world = plugin_world();
    world.plugins.then([PluginMessage::Answer(json!({
        "items": [{ "trigger": "plugin_fn", "contents": "plugin_fn()", "format": 0, "kind": [3, "f", "Function"] }],
        "flags": 0, "pending": []
    }))]);
    let mut editor = typed(&world, "/p/a.py", "pl");
    assert_eq!(triggers(&editor), vec!["plugin_fn", "plugh"]);
    let (name, view, args) = world.plugins.queries.borrow().last().cloned().unwrap();
    assert_eq!((name.as_str(), view, args), ("on_query_completions", editor.active_document().map(|d| d.id), json!(["pl", [8]])));

    editor.hide_auto_complete();
    world.plugins.then([PluginMessage::Answer(json!({ "items": ["plugin_word"], "flags": 8, "pending": [] }))]);
    editor.show_auto_complete(AutoCompleteOptions::default());
    assert_eq!(triggers(&editor), vec!["plugin_word"], "INHIBIT_WORD_COMPLETIONS");
}

#[test]
fn a_completion_list_filled_later_shows_up_when_it_arrives() {
    let world = plugin_world();
    world.plugins.then([PluginMessage::Answer(json!({ "items": [], "flags": 0, "pending": [7] }))]);
    let mut editor = typed(&world, "/p/a.py", "pl");
    assert_eq!(triggers(&editor), vec!["plugh"], "the words meanwhile");

    world.plugins.meanwhile([plugin_call("completion_list_set", json!([7, ["plover"], 0]))]);
    editor.poll_plugins(&mut |_, _| {});
    assert_eq!(triggers(&editor), vec!["plover", "plugh"]);
}

#[test]
fn a_command_completion_runs_its_command() {
    let world = plugin_world();
    world.plugins.then([PluginMessage::Answer(json!({
        "items": [{ "trigger": "plotit", "contents": "plot {\"kind\": \"bar\"}", "format": 2 }], "flags": 8, "pending": []
    }))]);
    let mut editor = typed(&world, "/p/a.py", "x pl");
    editor.commit_completion(None);
    assert_eq!(active_text(&editor), "plugh\nx ", "the typed word goes");
    let mut ran = Vec::new();
    editor.poll_plugins(&mut |_, command: &Command| ran.push(command.clone()));
    assert_eq!(ran, vec![Command::with_args("plot", json!({ "kind": "bar" }))]);
}

#[test]
fn plugins_ask_whether_it_shows_and_for_the_words() {
    let world = World::new().with_file("/p/a.py", "alpha beta\n").with_plugin_commands(&[("probe", Text)]);
    let mut editor = typed(&world, "/p/a.py", "al");
    let v = editor.active_document().unwrap().id;
    world.plugins.then([
        plugin_call("view_is_auto_complete_visible", json!([v])),
        plugin_call("view_extract_completions", json!([v, "a", -1])),
        PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    assert_eq!(*world.plugins.replies.borrow(), vec![Ok(json!(true)), Ok(json!(["alpha"]))]);
}
