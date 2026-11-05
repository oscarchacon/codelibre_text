//! Auto-complete entries: what a completion is, how Sublime writes them (plugins' lists,
//! `.sublime-completions` files, snippets with a `tabTrigger`), and the words of a text, which are
//! completions too.

use serde_json::Value;

use crate::text::is_word_char;

/// How a completion's contents are inserted (Sublime's `COMPLETION_FORMAT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionFormat {
    /// As written.
    Text,
    /// A snippet: `$1`, `${1:default}`, `$SELECTION`… (see `text_commands::insert_snippet`).
    #[default]
    Snippet,
    /// `contents` is a command (`name {"arg": …}`, as `sublime.format_command` writes it): the
    /// typed prefix goes and the command runs.
    Command,
}

impl CompletionFormat {
    /// Sublime's numbers: text 0, snippet 1, command 2.
    pub fn from_number(number: u64) -> Self {
        match number {
            0 => Self::Text,
            2 => Self::Command,
            _ => Self::Snippet,
        }
    }
}

/// Where a completion came from: the list shows a plugin's and a package's before the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum CompletionSource {
    #[default]
    Plugin,
    /// A `.sublime-completions` file or a snippet.
    Package,
    /// A word of the text.
    Word,
}

/// Sublime's `kind`: what it is (`KindId`: 0 ambiguous, 1 keyword, 2 type, 3 function,
/// 4 namespace, 5 navigation, 6 markup, 7 variable, 8 snippet, 9… colors), a letter for the badge
/// and a name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionKind {
    pub id: u8,
    pub letter: String,
    pub name: String,
}

impl CompletionKind {
    pub fn snippet() -> Self {
        Self::named("snippet").unwrap_or_default()
    }

    /// A kind as `.sublime-completions` files name it (`"snippet"`, `"function"`…).
    pub fn named(name: &str) -> Option<Self> {
        let (id, letter, name) = match name {
            "ambiguous" => (0, "", ""),
            "keyword" => (1, "k", "Keyword"),
            "type" => (2, "t", "Type"),
            "function" => (3, "f", "Function"),
            "namespace" => (4, "a", "Namespace"),
            "navigation" => (5, "n", "Navigation"),
            "markup" => (6, "m", "Markup"),
            "variable" => (7, "v", "Variable"),
            "snippet" => (8, "s", "Snippet"),
            _ => return None,
        };
        Some(Self { id, letter: letter.into(), name: name.into() })
    }

    /// `[id, letter, name]` (a plugin's tuple) or a name.
    fn from_json(value: &Value) -> Self {
        match value {
            Value::String(name) => Self::named(name).unwrap_or_default(),
            Value::Array(parts) => Self {
                id: parts.first().and_then(Value::as_u64).unwrap_or(0).min(u8::MAX as u64) as u8,
                letter: parts.get(1).and_then(Value::as_str).unwrap_or("").into(),
                name: parts.get(2).and_then(Value::as_str).unwrap_or("").into(),
            },
            _ => Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Completion {
    /// What the typed prefix is matched against, and what the list shows.
    pub trigger: String,
    /// Shown at the right of the list.
    pub annotation: String,
    pub contents: String,
    pub format: CompletionFormat,
    pub kind: CompletionKind,
    /// minihtml shown under the list for the selected completion.
    pub details: String,
    /// Sublime's `COMPLETION_FLAG_KEEP_PREFIX`: inserted after the prefix instead of replacing it.
    pub keep_prefix: bool,
    pub source: CompletionSource,
}

impl Completion {
    /// A word of the text.
    pub fn word(word: &str) -> Self {
        Self {
            trigger: word.into(),
            contents: word.into(),
            format: CompletionFormat::Text,
            source: CompletionSource::Word,
            ..Self::default()
        }
    }

    /// One completion as Sublime writes them: a string (trigger and contents), a list
    /// `[trigger, contents]` whose trigger may end in `\t<annotation>`, or an object with
    /// `trigger`, `contents` (or `completion`), `annotation`, `kind`, `details`, `format` (or
    /// `completion_format`) and `flags`.
    pub fn from_json(value: &Value, source: CompletionSource) -> Option<Self> {
        let with_hint = |trigger: &str, contents: &str| {
            let (trigger, annotation) = trigger.split_once('\t').unwrap_or((trigger, ""));
            Self {
                trigger: trigger.into(),
                annotation: annotation.into(),
                contents: contents.into(),
                source,
                ..Self::default()
            }
        };
        match value {
            Value::String(text) => Some(with_hint(text, text.split_once('\t').map_or(text, |(t, _)| t))),
            Value::Array(pair) => {
                let trigger = pair.first()?.as_str()?;
                let contents = pair.get(1).and_then(Value::as_str).unwrap_or_else(|| trigger.split('\t').next().unwrap_or(""));
                Some(with_hint(trigger, contents))
            }
            Value::Object(object) => {
                let text = |key: &str| object.get(key).and_then(Value::as_str);
                let trigger = text("trigger")?;
                let mut completion = with_hint(trigger, text("contents").or(text("completion")).unwrap_or(trigger));
                if let Some(annotation) = text("annotation") {
                    completion.annotation = annotation.into();
                }
                completion.details = text("details").unwrap_or("").into();
                if let Some(kind) = object.get("kind") {
                    completion.kind = CompletionKind::from_json(kind);
                }
                if let Some(format) = object.get("format").or(object.get("completion_format")).and_then(Value::as_u64) {
                    completion.format = CompletionFormat::from_number(format);
                }
                completion.keep_prefix = object.get("flags").and_then(Value::as_u64).is_some_and(|f| f & 1 != 0);
                Some(completion)
            }
            _ => None,
        }
    }
}

/// The completions of a `.sublime-completions` file and the scope selector they apply to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompletionSet {
    pub scope: String,
    pub completions: Vec<Completion>,
}

impl CompletionSet {
    /// `{"scope": "…", "completions": […]}` (a `.sublime-completions` file).
    pub fn from_completions_file(value: &Value) -> Self {
        let completions = value.get("completions").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        Self {
            scope: value.get("scope").and_then(Value::as_str).unwrap_or("").into(),
            completions: completions.iter().filter_map(|c| Completion::from_json(c, CompletionSource::Package)).collect(),
        }
    }

    /// A `.sublime-snippet` (XML: `content`, `tabTrigger`, `scope`, `description`): a completion
    /// whose trigger is its `tabTrigger`; `None` without one (only key bindings can insert it).
    pub fn from_snippet(xml: &str) -> Option<Self> {
        let trigger = xml_element(xml, "tabTrigger").filter(|t| !t.trim().is_empty())?;
        let contents = xml_element(xml, "content")?;
        let description = xml_element(xml, "description").unwrap_or_default();
        let completion = Completion {
            annotation: if description.is_empty() { trigger.clone() } else { description },
            trigger: trigger.trim().into(),
            contents,
            format: CompletionFormat::Snippet,
            kind: CompletionKind::snippet(),
            source: CompletionSource::Package,
            ..Completion::default()
        };
        Some(Self {
            scope: xml_element(xml, "scope").unwrap_or_default().trim().into(),
            completions: vec![completion],
        })
    }
}

/// The text of the first `<name>` element: CDATA as it is, entities decoded elsewhere.
fn xml_element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    let inner = &xml[start..end];
    let mut text = String::new();
    let mut rest = inner;
    while let Some(at) = rest.find("<![CDATA[") {
        text.push_str(&decode_xml_entities(&rest[..at]));
        let data = &rest[at + "<![CDATA[".len()..];
        let close = data.find("]]>").unwrap_or(data.len());
        text.push_str(&data[..close]);
        rest = data.get(close + 3..).unwrap_or("");
    }
    text.push_str(&decode_xml_entities(rest));
    Some(text)
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The distinct words of `text` (letters, digits, `_`; not only digits; at least two chars),
/// nearest to the char `caret` first, without `exclude` (the word being typed). At most `limit`.
pub fn words_near(text: &str, caret: usize, exclude: &str, limit: usize) -> Vec<String> {
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut start: Option<(usize, usize)> = None; // (byte, char) where the current word began
    for (chars, (byte, c)) in text.char_indices().chain(std::iter::once((text.len(), ' '))).enumerate() {
        if is_word_char(c) {
            start.get_or_insert((byte, chars));
        } else if let Some((begin, begin_char)) = start.take() {
            let word = &text[begin..byte];
            if word.chars().nth(1).is_some() && !word.chars().all(|c| c.is_ascii_digit()) && word != exclude {
                let distance = if caret < begin_char { begin_char - caret } else { caret.saturating_sub(chars) };
                words.push((distance, word));
            }
        }
    }
    words.sort_by_key(|&(distance, _)| distance);
    let mut seen = std::collections::HashSet::new();
    words.into_iter().map(|(_, w)| w).filter(|w| seen.insert(*w)).take(limit).map(String::from).collect()
}

/// A command completion's contents (`name {"arg": …}`, or just `name`) as name and arguments.
pub fn parse_command_completion(contents: &str) -> (String, Value) {
    let contents = contents.trim();
    match contents.split_once(char::is_whitespace) {
        Some((name, args)) => (name.into(), serde_json::from_str(args.trim()).unwrap_or(Value::Null)),
        None => (contents.into(), Value::Null),
    }
}
