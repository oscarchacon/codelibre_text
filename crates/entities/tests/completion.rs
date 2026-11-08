use entities::completion::{
    parse_command_completion, words_near, Completion, CompletionFormat, CompletionKind, CompletionSet, CompletionSource,
};
use serde_json::json;

#[test]
fn completions_are_read_in_every_form_sublime_writes_them() {
    let plain = Completion::from_json(&json!("print"), CompletionSource::Plugin).unwrap();
    assert_eq!((plain.trigger.as_str(), plain.contents.as_str(), plain.format), ("print", "print", CompletionFormat::Snippet));

    let pair = Completion::from_json(&json!(["fn\tfunction", "fn ${1:name}() {}"]), CompletionSource::Plugin).unwrap();
    assert_eq!((pair.trigger.as_str(), pair.annotation.as_str(), pair.contents.as_str()), ("fn", "function", "fn ${1:name}() {}"));

    let item = json!({
        "trigger": "len", "annotation": "builtin", "contents": "len($1)", "format": 0,
        "kind": [3, "f", "Function"], "details": "<b>len</b>", "flags": 1
    });
    let item = Completion::from_json(&item, CompletionSource::Plugin).unwrap();
    assert_eq!(item.format, CompletionFormat::Text);
    assert_eq!(item.kind, CompletionKind { id: 3, letter: "f".into(), name: "Function".into() });
    assert_eq!((item.details.as_str(), item.keep_prefix), ("<b>len</b>", true));

    assert!(Completion::from_json(&json!(3), CompletionSource::Plugin).is_none());
}

#[test]
fn a_completions_file_has_a_scope_and_kinds_by_name() {
    let file = json!({
        "scope": "source.python",
        "completions": [
            "self",
            { "trigger": "def", "contents": "def ${1:f}():", "kind": "snippet", "annotation": "function" },
        ],
    });
    let set = CompletionSet::from_completions_file(&file);
    assert_eq!(set.scope, "source.python");
    assert_eq!(set.completions.len(), 2);
    assert_eq!(set.completions[1].kind, CompletionKind::snippet());
    assert!(set.completions.iter().all(|c| c.source == CompletionSource::Package));
}

#[test]
fn a_snippet_with_a_tab_trigger_is_a_completion() {
    let xml = r#"<snippet>
    <content><![CDATA[for ${1:x} in ${2:items}:
	${0:pass}]]></content>
    <tabTrigger>for</tabTrigger>
    <scope>source.python</scope>
    <description>For Loop &amp; more</description>
</snippet>"#;
    let set = CompletionSet::from_snippet(xml).unwrap();
    assert_eq!(set.scope, "source.python");
    let snippet = &set.completions[0];
    assert_eq!((snippet.trigger.as_str(), snippet.annotation.as_str()), ("for", "For Loop & more"));
    assert_eq!(snippet.contents, "for ${1:x} in ${2:items}:\n\t${0:pass}");
    assert_eq!(snippet.kind, CompletionKind::snippet());

    assert!(CompletionSet::from_snippet("<snippet><content>x</content></snippet>").is_none(), "no tabTrigger");
}

#[test]
fn words_are_listed_once_nearest_first_without_the_typed_one() {
    let text = "far_word alpha\nbeta al alpha 42 x gamma_2";
    let caret = text.find(" al ").unwrap() + 3;
    assert_eq!(words_near(text, caret, "al", 10), vec!["alpha", "beta", "gamma_2", "far_word"]);
    assert_eq!(words_near(text, caret, "al", 2), vec!["alpha", "beta"]);
}

#[test]
fn command_completions_name_their_arguments() {
    assert_eq!(parse_command_completion("goto_line {\"line\": 3}"), ("goto_line".into(), json!({ "line": 3 })));
    assert_eq!(parse_command_completion("save"), ("save".into(), serde_json::Value::Null));
}
