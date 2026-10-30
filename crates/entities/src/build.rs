//! Build systems (`.sublime-build`) and the results of their output: which build applies to a
//! file, how `$file` and friends expand, and which output lines point at a place in a file.

use std::collections::HashMap;

use regex::Regex;
use serde_json::{Map, Value};

/// The keys of a `.sublime-build` that choose a build; all the others are its target's arguments.
const SELECTION_KEYS: &[&str] = &["selector", "file_patterns", "keyfiles", "variants", "target", "cancel", "name", "windows", "osx", "linux"];
/// Arguments that are regular expressions: their `$` is not a variable.
const REGEX_KEYS: &[&str] = &["file_regex", "line_regex"];

/// A `.sublime-build` file, with its platform's overrides already applied.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildSystem {
    /// The resource it came from (`Packages/Python/Python.sublime-build`).
    pub resource: String,
    /// The file name without its extension.
    pub name: String,
    pub selector: String,
    pub file_patterns: Vec<String>,
    pub keyfiles: Vec<String>,
    pub variants: Vec<BuildVariant>,
    /// The command that runs it (`exec` unless it says otherwise).
    pub target: String,
    /// How to cancel it: a command's name or the target's arguments.
    pub cancel: Option<Value>,
    /// The target's arguments.
    pub arguments: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildVariant {
    pub name: String,
    /// Replace the base build's arguments of the same name.
    pub arguments: Map<String, Value>,
}

impl BuildSystem {
    /// `platform` is Sublime's name for it (`Windows`, `OSX`, `Linux`). `None` if `json` is not an object.
    pub fn from_json(resource: &str, json: &Value, platform: &str) -> Option<Self> {
        let merged = for_platform(json.as_object()?, platform);
        let strings = |key: &str| -> Vec<String> {
            let items = merged.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            items.iter().filter_map(Value::as_str).map(String::from).collect()
        };
        let file_name = resource.rsplit('/').next().unwrap_or(resource);
        let variants = merged.get("variants").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        Some(Self {
            resource: resource.to_string(),
            name: file_name.strip_suffix(".sublime-build").unwrap_or(file_name).to_string(),
            selector: merged.get("selector").and_then(Value::as_str).unwrap_or_default().to_string(),
            file_patterns: strings("file_patterns"),
            keyfiles: strings("keyfiles"),
            variants: variants.iter().filter_map(|v| BuildVariant::from_json(v, platform)).collect(),
            target: merged.get("target").and_then(Value::as_str).unwrap_or("exec").to_string(),
            cancel: merged.get("cancel").cloned(),
            arguments: arguments(&merged),
        })
    }

    /// The target's arguments, with those of the variant `name` if it has one.
    pub fn arguments_for(&self, variant: Option<&str>) -> Map<String, Value> {
        let mut arguments = self.arguments.clone();
        if let Some(variant) = self.variants.iter().find(|v| Some(v.name.as_str()) == variant) {
            arguments.extend(variant.arguments.clone());
        }
        arguments
    }
}

impl BuildVariant {
    fn from_json(json: &Value, platform: &str) -> Option<Self> {
        let merged = for_platform(json.as_object()?, platform);
        Some(Self {
            name: merged.get("name")?.as_str()?.to_string(),
            arguments: arguments(&merged),
        })
    }
}

/// `json` with the object under its platform's key on top.
fn for_platform(json: &Map<String, Value>, platform: &str) -> Map<String, Value> {
    let mut merged = json.clone();
    if let Some(own) = json.get(&platform.to_lowercase()).and_then(Value::as_object) {
        merged.extend(own.clone());
    }
    merged
}

fn arguments(merged: &Map<String, Value>) -> Map<String, Value> {
    merged.iter().filter(|(key, _)| !SELECTION_KEYS.contains(&key.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Whether a scope selector matches the scope of a file (`source.python`). Alternatives go with
/// `,` or `|`, spaces mean "and", ` - ` excludes.
pub fn selector_matches(selector: &str, scope: &str) -> bool {
    let has = |wanted: &str| scope.split_whitespace().any(|s| s == wanted || s.strip_prefix(wanted).is_some_and(|r| r.starts_with('.')));
    let alternative = |text: &str| {
        let mut parts = text.split(" - ");
        let wanted = parts.next().unwrap_or_default().split_whitespace().collect::<Vec<_>>();
        !wanted.is_empty() && wanted.iter().all(|w| has(w)) && parts.all(|excluded| !has(excluded.trim()))
    };
    selector.split([',', '|']).any(alternative)
}

/// Expands `$name`, `${name}`, `${name:default}` and `${name/regex/replacement/}`. `\$` is a
/// literal `$`. Names not in `variables` stay as they are: they may be the shell's (`$HOME`).
pub fn expand_variables(text: &str, variables: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(['$', '\\']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("\\$") {
            out.push('$');
            rest = after;
        } else if let Some(after) = tail.strip_prefix('\\') {
            out.push('\\');
            rest = after;
        } else if let Some((value, after)) = expand_one(&tail[1..], variables) {
            out.push_str(&value);
            rest = after;
        } else {
            out.push('$');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// The value of the variable at the start of `text` (what follows a `$`) and the text after it.
fn expand_one<'a>(text: &'a str, variables: &HashMap<String, String>) -> Option<(String, &'a str)> {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if let Some(inner) = text.strip_prefix('{') {
        let end = closing_brace(inner)?;
        let body = &inner[..end];
        let name_end = body.find(|c| !is_name(c)).unwrap_or(body.len());
        let (name, modifier) = body.split_at(name_end);
        let value = match (variables.get(name), modifier.chars().next()) {
            (Some(value), Some('/')) => substitute(value, &modifier[1..]),
            (Some(value), _) => value.clone(),
            (None, Some(':')) => expand_variables(&modifier[1..], variables),
            (None, _) => return None,
        };
        return Some((value, &inner[end + 1..]));
    }
    let end = text.find(|c| !is_name(c)).unwrap_or(text.len());
    Some((variables.get(&text[..end])?.clone(), &text[end..]))
}

/// The `}` that closes a `${`, skipping nested ones.
fn closing_brace(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => return Some(i),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// `regex/replacement/` applied to `value`.
fn substitute(value: &str, spec: &str) -> String {
    let mut parts = spec.splitn(3, '/');
    let (pattern, replacement) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    match Regex::new(pattern) {
        Ok(regex) => regex.replace_all(value, replacement).into_owned(),
        Err(_) => value.to_string(),
    }
}

/// Every string inside `value` expanded, except regular expressions.
pub fn expand_arguments(value: &Value, variables: &HashMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(expand_variables(text, variables)),
        Value::Array(items) => Value::Array(items.iter().map(|v| expand_arguments(v, variables)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| {
                    let v = if REGEX_KEYS.contains(&key.as_str()) { v.clone() } else { expand_arguments(v, variables) };
                    (key.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A line of output that points at a place in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultLocation {
    /// The line of the output (from 0).
    pub line: usize,
    /// As the output wrote it: may be relative to the build's folder.
    pub file: String,
    /// The line in the file (from 1).
    pub row: usize,
    /// The column in the file (from 1).
    pub column: usize,
    pub message: String,
}

/// The results in `text`. `file_regex` captures file, line, column, message; `line_regex` captures
/// line, column, message for lines under a file named earlier. Only the file is required: alone,
/// it is the file of the next `line_regex` lines, or a result on line 1 if there is no `line_regex`.
pub fn find_results(text: &str, file_regex: &str, line_regex: &str) -> Result<Vec<ResultLocation>, regex::Error> {
    let file_regex = (!file_regex.is_empty()).then(|| Regex::new(file_regex)).transpose()?;
    let line_regex = (!line_regex.is_empty()).then(|| Regex::new(line_regex)).transpose()?;
    let mut results = Vec::new();
    let mut current_file = None;
    for (line, content) in text.lines().enumerate() {
        if let Some(captures) = file_regex.as_ref().and_then(|r| r.captures(content)) {
            let group = |i: usize| captures.get(i).map_or("", |m| m.as_str());
            if group(1).is_empty() {
                continue;
            }
            current_file = Some(group(1).to_string());
            if !group(2).is_empty() || line_regex.is_none() {
                results.push(location(line, group(1), group(2), group(3), group(4)));
            }
        } else if let (Some(captures), Some(file)) = (line_regex.as_ref().and_then(|r| r.captures(content)), &current_file) {
            let group = |i: usize| captures.get(i).map_or("", |m| m.as_str());
            results.push(location(line, file, group(1), group(2), group(3)));
        }
    }
    Ok(results)
}

fn location(line: usize, file: &str, row: &str, column: &str, message: &str) -> ResultLocation {
    ResultLocation {
        line,
        file: file.to_string(),
        row: row.parse().unwrap_or(1).max(1),
        column: column.parse().unwrap_or(1).max(1),
        message: message.to_string(),
    }
}
