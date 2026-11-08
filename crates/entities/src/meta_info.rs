//! TextMate metadata (`.tmPreferences`): settings that apply where a scope selector matches,
//! such as `shellVariables` (`TM_COMMENT_START`…), `showInSymbolList` and
//! `symbolTransformation`. Sublime's `view.meta_info(key, point)` and its symbol lists read them.

use regex::Regex;
use serde_json::{Map, Value};

use crate::selector::score_selector;

/// One `.tmPreferences` file: where it applies and its `settings`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetaPreference {
    /// The resource it came from (`Packages/Python/Symbol List.tmPreferences`).
    pub name: String,
    /// Its `scope`: a selector (empty: everywhere).
    pub selector: String,
    pub settings: Map<String, Value>,
}

impl MetaPreference {
    /// Reads the plist value of a `.tmPreferences` file (`scope` and `settings`).
    pub fn from_plist(name: &str, plist: &Value) -> Option<Self> {
        let settings = plist.get("settings")?.as_object()?.clone();
        let selector = plist.get("scope").and_then(Value::as_str).unwrap_or_default().to_string();
        Some(Self { name: name.to_string(), selector, settings })
    }
}

/// The value of `key` for the scope `scope` (space-separated names): that of the preference whose
/// selector matches best; at equal scores, the one read last (a later package overrides).
pub fn meta_value<'a>(preferences: &'a [MetaPreference], scope: &str, key: &str) -> Option<&'a Value> {
    let mut best: Option<(u64, &Value)> = None;
    for preference in preferences {
        let Some(value) = preference.settings.get(key) else { continue };
        let score = score_selector(scope, &preference.selector);
        if score > 0 && best.is_none_or(|(s, _)| score >= s) {
            best = Some((score, value));
        }
    }
    best.map(|(_, value)| value)
}

/// A `shellVariables` entry (`[{"name": "TM_COMMENT_START", "value": "# "}]`) for `scope`.
pub fn shell_variable(preferences: &[MetaPreference], scope: &str, name: &str) -> Option<String> {
    let variables = meta_value(preferences, scope, "shellVariables")?.as_array()?;
    variables.iter().find(|v| v.get("name").and_then(Value::as_str) == Some(name))?.get("value")?.as_str().map(String::from)
}

/// Whether a plist setting is on: `1`, `"1"`, `true`.
pub fn is_on(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
        Value::String(s) => !matches!(s.trim(), "" | "0" | "false" | "NO"),
        _ => false,
    }
}

/// The selectors where `key` is on (`showInSymbolList`, `showInIndexedSymbolList`…), joined
/// with `,`. Empty when no preference sets it.
pub fn selectors_where_on(preferences: &[MetaPreference], key: &str) -> String {
    let on: Vec<&str> = preferences
        .iter()
        .filter(|p| p.settings.get(key).is_some_and(is_on))
        .map(|p| p.selector.trim())
        .filter(|s| !s.is_empty())
        .collect();
    on.join(", ")
}

/// Applies a `symbolTransformation` (TextMate's `s/regex/format/flags;` steps, one or more,
/// separated by `;` or new lines, `#` comments) to a symbol's text. A step that does not parse or
/// whose regex this engine cannot compile is skipped.
pub fn transform_symbol(text: &str, transformation: &str) -> String {
    let mut result = text.to_string();
    for (pattern, format, global) in substitutions(transformation) {
        let Ok(regex) = Regex::new(&pattern) else { continue };
        let format = format_to_regex_replacement(&format);
        result = if global { regex.replace_all(&result, format.as_str()) } else { regex.replace(&result, format.as_str()) }.into_owned();
    }
    result
}

/// The `s/…/…/…` steps of a transformation: (regex, format, global).
fn substitutions(transformation: &str) -> Vec<(String, String, bool)> {
    let mut steps = Vec::new();
    let chars: Vec<char> = transformation.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            's' if chars.get(i + 1) == Some(&'/') => {
                i += 2;
                let mut parts = [String::new(), String::new()];
                for part in &mut parts {
                    while i < chars.len() && chars[i] != '/' {
                        if chars[i] == '\\' && chars.get(i + 1) == Some(&'/') {
                            part.push('/');
                            i += 2;
                            continue;
                        }
                        if chars[i] == '\\' && i + 1 < chars.len() {
                            part.push(chars[i]);
                            part.push(chars[i + 1]);
                            i += 2;
                            continue;
                        }
                        part.push(chars[i]);
                        i += 1;
                    }
                    i += 1;
                }
                let mut flags = String::new();
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    flags.push(chars[i]);
                    i += 1;
                }
                let [pattern, format] = parts;
                steps.push((pattern, format, flags.contains('g')));
            }
            _ => i += 1,
        }
    }
    steps
}

/// TextMate formats write groups `$1` and escapes `\n`; the regex crate wants `${1}`.
fn format_to_regex_replacement(format: &str) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' if chars.peek().is_some_and(char::is_ascii_digit) => {
                let mut digits = String::new();
                while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(*d);
                    chars.next();
                }
                out.push_str(&format!("${{{digits}}}"));
            }
            '$' => out.push_str("$$"),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    out
}

/// Sublime's kinds (`sublime.KIND_ID_*`): the id, letter and description a symbol shows with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolKind {
    pub id: u8,
    pub letter: char,
    pub description: &'static str,
}

/// The kind of a symbol from its scope, as Sublime guesses it when the syntax does not say.
pub fn symbol_kind(scope: &str) -> SymbolKind {
    let has = |name: &str| scope.split_whitespace().any(|s| s == name || s.starts_with(&format!("{name}.")));
    // `KIND_FUNCTION`, `KIND_TYPE`, `KIND_NAMESPACE`, `KIND_MARKUP`, `KIND_VARIABLE`, `KIND_AMBIGUOUS`.
    let (id, letter, description) = if has("entity.name.function") || has("entity.name.method") || has("entity.name.macro") {
        (3, 'f', "Function")
    } else if has("entity.name.class") || has("entity.name.struct") || has("entity.name.enum")
        || has("entity.name.trait") || has("entity.name.interface") || has("entity.name.type")
        || has("entity.name.union") || has("entity.name.impl")
    {
        (2, 't', "Type")
    } else if has("entity.name.namespace") || has("entity.name.module") || has("entity.name.package") {
        (4, 'a', "Namespace")
    } else if has("entity.name.section") || has("markup.heading") {
        (6, 'm', "Markup")
    } else if has("entity.name.constant") || has("entity.name.variable") || has("variable") {
        (7, 'v', "Variable")
    } else {
        (0, ' ', "")
    };
    SymbolKind { id, letter, description }
}
