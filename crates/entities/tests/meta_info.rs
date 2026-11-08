//! Tests for `meta_info` (the TextMate metadata of `.tmPreferences`).

use entities::meta_info::{meta_value, selectors_where_on, shell_variable, symbol_kind, transform_symbol, MetaPreference};
use serde_json::json;

fn preference(selector: &str, settings: serde_json::Value) -> MetaPreference {
    MetaPreference { name: String::new(), selector: selector.into(), settings: settings.as_object().cloned().unwrap() }
}

#[test]
fn the_best_matching_preference_answers() {
    let preferences = vec![
        preference("source", json!({ "shellVariables": [{ "name": "TM_COMMENT_START", "value": "// " }] })),
        preference("source.python", json!({ "shellVariables": [{ "name": "TM_COMMENT_START", "value": "# " }] })),
        preference("text", json!({ "other": 1 })),
    ];
    assert_eq!(shell_variable(&preferences, "source.python string", "TM_COMMENT_START").as_deref(), Some("# "));
    assert_eq!(shell_variable(&preferences, "source.rust", "TM_COMMENT_START").as_deref(), Some("// "));
    assert_eq!(meta_value(&preferences, "source.python", "other"), None, "text does not match");
    assert_eq!(meta_value(&preferences, "text.plain", "other"), Some(&json!(1)));
}

#[test]
fn symbol_list_selectors_and_transformations() {
    let preferences = vec![
        preference("source.python entity.name.function", json!({ "showInSymbolList": 1 })),
        preference("source.python entity.name.class", json!({ "showInSymbolList": "1" })),
        preference("source.python meta.call", json!({ "showInSymbolList": 0 })),
    ];
    assert_eq!(selectors_where_on(&preferences, "showInSymbolList"), "source.python entity.name.function, source.python entity.name.class");
    assert_eq!(transform_symbol("  helper", r"s/^\s*//;"), "helper");
    assert_eq!(transform_symbol("get_x", "s/_(\\w)/ $1/g;\n# comment\ns/^/fn /"), "fn get x");
    assert_eq!(transform_symbol("kept", "s/(?<=k)e/E/"), "kept", "a regex this engine lacks is skipped");
}

#[test]
fn kinds_follow_the_scope() {
    assert_eq!(symbol_kind("source.rust entity.name.function.rust").letter, 'f');
    assert_eq!(symbol_kind("source.rust entity.name.struct.rust").id, 2);
    assert_eq!(symbol_kind("text.html.markdown markup.heading entity.name.section").description, "Markup");
    assert_eq!(symbol_kind("source.c").id, 0);
}
