//! Tests for `gateways::sublime_json` (Sublime Text's JSON dialect).

use interface_adapters::gateways::sublime_json::parse;
use serde_json::json;

#[test]
fn plain_json_is_accepted() {
    assert_eq!(parse(r#"{"a": [1, 2], "b": null}"#).unwrap(), json!({ "a": [1, 2], "b": null }));
}

#[test]
fn line_and_block_comments_are_ignored() {
    let source = "{\n  // a comment\n  \"a\": 1, /* inline */ \"b\": 2\n  /* multi\n     line */\n}";
    assert_eq!(parse(source).unwrap(), json!({ "a": 1, "b": 2 }));
}

#[test]
fn trailing_commas_are_allowed_in_objects_and_arrays() {
    assert_eq!(parse("{ \"a\": [1, 2,], }").unwrap(), json!({ "a": [1, 2] }));
}

#[test]
fn a_trailing_comma_followed_by_a_comment_is_allowed() {
    assert_eq!(parse("[1, // last\n]").unwrap(), json!([1]));
    assert_eq!(parse("[1, /* last */ ]").unwrap(), json!([1]));
}

#[test]
fn comment_markers_inside_strings_are_kept() {
    let value = parse(r#"{ "url": "http://x/*y*/", "c": "a, ]" }"#).unwrap();
    assert_eq!(value["url"], "http://x/*y*/");
    assert_eq!(value["c"], "a, ]");
}

#[test]
fn escaped_quotes_do_not_end_strings() {
    let value = parse(r#"{ "s": "say \"hi\" // not a comment" }"#).unwrap();
    assert_eq!(value["s"], "say \"hi\" // not a comment");
}

#[test]
fn a_byte_order_mark_is_skipped() {
    assert_eq!(parse("\u{feff}{}").unwrap(), json!({}));
}

#[test]
fn invalid_json_is_an_error() {
    assert!(parse("{ \"a\": }").is_err());
    assert!(parse("").is_err());
}
