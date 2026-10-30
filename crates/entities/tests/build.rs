//! Tests for build systems and output results (`entities::build`).

use std::collections::HashMap;

use entities::build::{expand_arguments, expand_variables, find_results, selector_matches, BuildSystem, ResultLocation};
use serde_json::json;

fn variables() -> HashMap<String, String> {
    [("file", "/p/a.py"), ("file_base_name", "a"), ("empty", "")].map(|(k, v)| (k.to_string(), v.to_string())).into()
}

#[test]
fn a_build_file_reads_its_selection_and_arguments() {
    let json = json!({
        "selector": "source.python",
        "file_patterns": ["*.py"],
        "keyfiles": ["setup.py"],
        "shell_cmd": "python $file",
        "file_regex": "^x$",
        "variants": [{ "name": "Run", "shell_cmd": "python -u $file" }],
    });
    let build = BuildSystem::from_json("Packages/Python/Python.sublime-build", &json, "Linux").unwrap();
    assert_eq!(build.name, "Python");
    assert_eq!(build.selector, "source.python");
    assert_eq!(build.file_patterns, ["*.py"]);
    assert_eq!(build.keyfiles, ["setup.py"]);
    assert_eq!(build.target, "exec");
    assert_eq!(build.arguments.len(), 2, "only the target's arguments: {:?}", build.arguments);
    assert_eq!(build.arguments_for(None)["shell_cmd"], "python $file");
    let run = build.arguments_for(Some("Run"));
    assert_eq!(run["shell_cmd"], "python -u $file");
    assert_eq!(run["file_regex"], "^x$", "the rest comes from the base build");
}

#[test]
fn the_platform_overrides_a_build_and_its_variants() {
    let json = json!({
        "cmd": ["make"],
        "windows": { "cmd": ["nmake"] },
        "variants": [{ "name": "Clean", "cmd": ["make", "clean"], "windows": { "cmd": ["nmake", "clean"] } }],
    });
    let windows = BuildSystem::from_json("a.sublime-build", &json, "Windows").unwrap();
    assert_eq!(windows.arguments["cmd"], json!(["nmake"]));
    assert_eq!(windows.arguments_for(Some("Clean"))["cmd"], json!(["nmake", "clean"]));
    let linux = BuildSystem::from_json("a.sublime-build", &json, "Linux").unwrap();
    assert_eq!(linux.arguments["cmd"], json!(["make"]));
}

#[test]
fn a_build_that_is_not_an_object_is_refused() {
    assert_eq!(BuildSystem::from_json("a.sublime-build", &json!([1]), "Linux"), None);
}

#[test]
fn selectors_match_by_scope_prefix() {
    assert!(selector_matches("source.python", "source.python"));
    assert!(selector_matches("source", "source.python"));
    assert!(!selector_matches("source.py", "source.python"), "only whole parts of the name");
    assert!(selector_matches("text.html, source.python", "source.python"));
    assert!(selector_matches("text.html | source.python", "source.python"));
    assert!(!selector_matches("source - source.python", "source.python"));
    assert!(!selector_matches("", "source.python"));
}

#[test]
fn variables_expand_in_all_their_forms() {
    let v = variables();
    assert_eq!(expand_variables("run $file now", &v), "run /p/a.py now");
    assert_eq!(expand_variables("${file_base_name}.exe", &v), "a.exe");
    assert_eq!(expand_variables("${missing:fallback}", &v), "fallback");
    assert_eq!(expand_variables("${missing:${file}}", &v), "/p/a.py");
    assert_eq!(expand_variables("${empty:unused}", &v), "");
    assert_eq!(expand_variables("${file/\\.py$/.txt/}", &v), "/p/a.txt");
}

#[test]
fn what_is_not_a_known_variable_stays() {
    let v = variables();
    assert_eq!(expand_variables("echo $HOME ${OTHER} 100$", &v), "echo $HOME ${OTHER} 100$");
    assert_eq!(expand_variables("\\$file C:\\dir", &v), "$file C:\\dir");
}

#[test]
fn arguments_expand_except_regular_expressions() {
    let arguments = json!({ "cmd": ["python", "$file"], "file_regex": "^$file$", "quiet": true });
    let expanded = expand_arguments(&arguments, &variables());
    assert_eq!(expanded, json!({ "cmd": ["python", "/p/a.py"], "file_regex": "^$file$", "quiet": true }));
}

#[test]
fn file_regex_finds_the_file_line_column_and_message() {
    let text = "compiling\nsrc/a.rs:3:7: error: no\nok\nb.rs:9:: warning\n";
    let results = find_results(text, r"^(.+?):(\d+):(\d*):? ?(.*)$", "").unwrap();
    assert_eq!(
        results,
        [
            ResultLocation { line: 1, file: "src/a.rs".into(), row: 3, column: 7, message: "error: no".into() },
            ResultLocation { line: 3, file: "b.rs".into(), row: 9, column: 1, message: "warning".into() },
        ]
    );
}

#[test]
fn line_regex_uses_the_last_file_named() {
    let text = "a.py\n  3: bad\n  4: worse\nb.py\n  1: no\n";
    let results = find_results(text, r"^(\w+\.py)$", r"^\s+(\d+)(): (.*)$").unwrap();
    let found: Vec<(&str, usize, &str)> = results.iter().map(|r| (r.file.as_str(), r.row, r.message.as_str())).collect();
    assert_eq!(found, [("a.py", 3, "bad"), ("a.py", 4, "worse"), ("b.py", 1, "no")]);
}

#[test]
fn a_line_regex_needs_a_file_above_it() {
    assert_eq!(find_results("3: bad\n", "^(never)$", r"^(\d+): (.*)$").unwrap(), []);
}

#[test]
fn a_file_alone_is_a_result_without_a_line_regex() {
    let results = find_results("see a.py\n", r"see (.*)", "").unwrap();
    assert_eq!((results[0].file.as_str(), results[0].row, results[0].column), ("a.py", 1, 1));
}

#[test]
fn a_bad_regular_expression_is_an_error() {
    assert!(find_results("x", "(", "").is_err());
}
