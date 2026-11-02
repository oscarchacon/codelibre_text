//! Tests for `entities::command`.

use entities::command::Command;
use serde_json::json;

#[test]
fn argument_getters_with_defaults() {
    let command = Command::with_args("x", json!({ "name": "a", "flag": true, "count": 3 }));
    assert_eq!(command.str_arg("name"), "a");
    assert_eq!(command.str_arg("missing"), "");
    assert!(command.bool_arg("flag", false));
    assert!(command.bool_arg("missing", true));
    assert_eq!(command.usize_arg("count", 0), 3);
    assert_eq!(command.usize_arg("missing", 7), 7);
}

#[test]
fn wrong_argument_types_fall_back_to_defaults() {
    let command = Command::with_args("x", json!({ "flag": "yes", "count": "3" }));
    assert!(!command.bool_arg("flag", false));
    assert_eq!(command.usize_arg("count", 1), 1);
}

#[test]
fn new_has_no_arguments() {
    let command = Command::new("save");
    assert_eq!(command.name, "save");
    assert!(command.args.is_null());
}

#[test]
fn same_as_treats_missing_and_empty_arguments_as_equal() {
    assert!(Command::new("save").same_as(&Command::with_args("save", json!({}))));
    assert!(Command::with_args("a", json!({ "x": 1 })).same_as(&Command::with_args("a", json!({ "x": 1 }))));
    assert!(!Command::with_args("a", json!({ "x": 1 })).same_as(&Command::with_args("a", json!({ "x": 2 }))));
    assert!(!Command::new("a").same_as(&Command::new("b")));
}
