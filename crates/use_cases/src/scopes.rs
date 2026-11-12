//! Use cases: the scopes of the text (from the syntax gateway), for plugins (`view.scope_name`,
//! `extract_scope`, `find_by_selector`…) and the `selector` key binding context. Points are chars.

use entities::document::{Document, DocumentId};
use entities::meta_info::{meta_value, selectors_where_on, shell_variable, symbol_kind, transform_symbol, SymbolKind};
use entities::selector::matches_selector;
use entities::text::{byte_to_char, char_to_byte};
use serde_json::Value;

use crate::editor::Editor;
use crate::gateways::{ContextFrame, ScopeSpan};

/// What a syntax shows in Goto Symbol when no `.tmPreferences` says: its names but those of tags
/// and labels.
const DEFAULT_SYMBOL_SELECTOR: &str = "entity.name - entity.name.tag - entity.name.label";

/// How many lines on each side of the point `extract_scope` reads first.
pub const EXTRACT_SCOPE_LINES: usize = 32;

/// A piece of the text with one scope, in chars of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedRange {
    pub start: usize,
    pub end: usize,
    pub scope: String,
}

/// One selection, as the key binding contexts see it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CaretContext {
    /// The scope at its caret (`selector`).
    pub scope: String,
    /// The scope at the end of its line (`eol_selector`: whether a string goes on past it).
    pub eol_scope: String,
    /// Its line before it (`preceding_text`) and after it (`following_text`).
    pub preceding_text: String,
    pub following_text: String,
    /// What it selects (`text`).
    pub text: String,
}

/// Which of a syntax's symbol lists (see [`Editor::symbol_regions`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolList {
    /// Goto Symbol, `view.symbol_regions()`.
    Symbols,
    /// Definitions for the index, `view.indexed_symbol_regions()`.
    Indexed,
    /// References for the index (`indexed_symbol_regions(SYMBOL_TYPE_REFERENCE)`).
    References,
}

/// A symbol of a document: Sublime's `SymbolRegion`. Points are chars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRegion {
    pub name: String,
    pub start: usize,
    pub end: usize,
    /// The scope at its start.
    pub scope: String,
    pub kind: SymbolKind,
}

/// Where a symbol of the window is (Sublime's `SymbolLocation`): a place in an open file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    /// The file's path (`None` for a view that is not a file).
    pub path: Option<std::path::PathBuf>,
    /// The path shown to the user: relative to the first open folder when the file is under it.
    pub display_name: String,
    /// Zero-based.
    pub row: usize,
    pub column: usize,
    pub syntax: String,
    pub kind: SymbolKind,
    /// From the references list (not a definition).
    pub reference: bool,
}

/// Which documents `window.symbol_locations` searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolSource {
    /// Both the index and the open files (they are the same here: no file is indexed on disk).
    Any,
    /// The open files that are saved (the index built while scanning is not kept).
    Index,
    OpenFiles,
}

/// Which symbols `window.symbol_locations` wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    Any,
    Definition,
    Reference,
}

impl Editor {
    /// The scope of the char at `point`, as Sublime writes it: names separated by spaces, with
    /// a space at the end. At the end of the text, the scope of the last char.
    pub fn scope_name(&self, id: DocumentId, point: usize) -> Option<String> {
        let doc = self.any_document(id)?;
        let byte = char_to_byte(&doc.text, point);
        let line = doc.text[..byte].matches('\n').count();
        let line_start = doc.text[..byte].rfind('\n').map_or(0, |at| at + 1);
        let spans = self.line_spans(doc, line);
        let column = byte - line_start;
        let span = spans.iter().find(|s| s.range.contains(&column)).or(spans.last());
        let scope = match span {
            Some(span) => span.scope.clone(),
            None if line > 0 && byte == doc.text.len() => return self.scope_name(id, point.saturating_sub(1)),
            None => self.gateways.syntaxes.base_scope(&doc.syntax),
        };
        Some(format!("{scope} "))
    }

    /// The scopes of the selections' carets in the active document (the `selector` context).
    pub fn caret_scopes(&self) -> Vec<String> {
        self.caret_contexts().into_iter().map(|c| c.scope).collect()
    }

    /// What the key binding contexts of each selection of the active document look at, in order.
    pub fn caret_contexts(&self) -> Vec<CaretContext> {
        let Some(doc) = self.active_document() else { return Vec::new() };
        let text = &doc.text;
        doc.selections()
            .iter()
            .map(|s| {
                let (start, end) = (char_to_byte(text, s.start()), char_to_byte(text, s.end()));
                let line_begin = text[..start].rfind('\n').map_or(0, |at| at + 1);
                let line_end = text[end..].find('\n').map_or(text.len(), |at| end + at);
                CaretContext {
                    scope: self.scope_name(doc.id, s.head).unwrap_or_default(),
                    eol_scope: self.scope_name(doc.id, byte_to_char(text, line_end)).unwrap_or_default(),
                    preceding_text: text[line_begin..start].to_string(),
                    following_text: text[end..line_end].to_string(),
                    text: text[start..end].to_string(),
                }
            })
            .collect()
    }

    /// Every piece of the text whose scope `selector` matches, neighbours joined.
    pub fn find_by_selector(&self, id: DocumentId, selector: &str) -> Vec<(usize, usize)> {
        let mut found: Vec<(usize, usize)> = Vec::new();
        for token in self.scoped_ranges(id, 0, usize::MAX) {
            if !matches_selector(&token.scope, selector) {
                continue;
            }
            match found.last_mut() {
                Some(last) if last.1 == token.start => last.1 = token.end,
                _ => found.push((token.start, token.end)),
            }
        }
        found
    }

    /// The text around `point` in the scope of its char, across lines: the neighbours with that
    /// scope or one inside it (a string with its quotes, a comment). Close to Sublime's extent of
    /// the innermost scope.
    ///
    /// Only the lines around `point` are read: a window of [`EXTRACT_SCOPE_LINES`] lines on each
    /// side, made four times wider while the scope goes on past one of its edges.
    pub fn extract_scope(&self, id: DocumentId, point: usize) -> Option<(usize, usize)> {
        let scope = self.scope_name(id, point)?;
        let doc = self.any_document(id)?;
        let line = doc.text[..char_to_byte(&doc.text, point)].matches('\n').count();
        let line_count = doc.text.split_inclusive('\n').count();
        let same = |t: &ScopedRange| t.scope == scope.trim_end() || t.scope.starts_with(scope.as_str());
        let mut reach = EXTRACT_SCOPE_LINES;
        loop {
            let lines = line.saturating_sub(reach)..(line + reach + 1).min(line_count.max(1));
            let tokens = self.line_tokens(doc, lines.clone());
            let at = tokens.iter().position(|t| t.start <= point && point < t.end).or(tokens.len().checked_sub(1))?;
            let first = tokens[..at].iter().rposition(|t| !same(t)).map_or(0, |i| i + 1);
            let last = tokens[at..].iter().position(|t| !same(t)).map_or(tokens.len(), |i| at + i);
            let cut_before = first == 0 && lines.start > 0;
            let cut_after = last == tokens.len() && lines.end < line_count;
            if !cut_before && !cut_after {
                return Some(if first < last { (tokens[first].start, tokens[last - 1].end) } else { (point, point) });
            }
            reach = reach.saturating_mul(4);
        }
    }

    /// The pieces of the text between `start` and `end` (chars) and their scopes, in order. Only
    /// the lines of that range are read from the highlighting cache (nothing is parsed again).
    pub fn scoped_ranges(&self, id: DocumentId, start: usize, end: usize) -> Vec<ScopedRange> {
        let Some(doc) = self.any_document(id) else { return Vec::new() };
        let first_line = doc.text[..char_to_byte(&doc.text, start)].matches('\n').count();
        let last_line = doc.text[..char_to_byte(&doc.text, end)].matches('\n').count();
        self.line_tokens(doc, first_line..last_line + 1)
            .into_iter()
            .filter(|t| t.end > start && t.start < end)
            .map(|t| ScopedRange { start: t.start.max(start), end: t.end.min(end), scope: t.scope })
            .collect()
    }

    /// The pieces of the lines `lines` and their scopes, in chars of the whole document.
    fn line_tokens(&self, doc: &Document, lines: std::ops::Range<usize>) -> Vec<ScopedRange> {
        let first_byte = if lines.start == 0 {
            0
        } else {
            doc.text.match_indices('\n').nth(lines.start - 1).map_or(doc.text.len(), |(at, _)| at + 1)
        };
        let scopes = self.gateways.syntaxes.lines_scopes(doc.id, &doc.text, &doc.syntax, lines);
        let (mut line_char, mut tokens) = (byte_to_char(&doc.text, first_byte), Vec::new());
        for (line_text, spans) in doc.text[first_byte..].split_inclusive('\n').zip(scopes) {
            for span in spans {
                let Some(text) = line_text.get(..span.range.end) else { continue };
                let a = line_char + line_text[..span.range.start].chars().count();
                let b = line_char + text.chars().count();
                tokens.push(ScopedRange { start: a, end: b, scope: span.scope });
            }
            line_char += line_text.chars().count();
        }
        tokens
    }

    /// `view.meta_info(key, point)`: the `.tmPreferences` setting `key` that applies at `point`.
    pub fn meta_info(&self, id: DocumentId, key: &str, point: usize) -> Option<Value> {
        let scope = self.scope_name(id, point)?;
        meta_value(&self.state.meta_preferences, scope.trim_end(), key).cloned()
    }

    /// A `shellVariables` value at `point` (`TM_COMMENT_START`…).
    pub fn shell_variable_at(&self, id: DocumentId, point: usize, name: &str) -> Option<String> {
        let scope = self.scope_name(id, point)?;
        shell_variable(&self.state.meta_preferences, scope.trim_end(), name)
    }

    /// `view.context_backtrace(point)`: the syntax's contexts at `point`, outermost first.
    pub fn context_backtrace(&self, id: DocumentId, point: usize) -> Vec<ContextFrame> {
        let Some(doc) = self.any_document(id) else { return Vec::new() };
        let byte = char_to_byte(&doc.text, point);
        let line = doc.text[..byte].matches('\n').count();
        let line_start = doc.text[..byte].rfind('\n').map_or(0, |at| at + 1);
        self.gateways.syntaxes.context_backtrace(doc.id, &doc.text, &doc.syntax, line, byte - line_start)
    }

    /// Some package's `.tmPreferences` says what goes in symbol lists.
    pub fn has_symbol_preferences(&self) -> bool {
        self.state.meta_preferences.iter().any(|p| p.settings.contains_key("showInSymbolList"))
    }

    /// The symbols of a document (`view.symbol_regions()`, Goto Symbol): the text of the runs the
    /// `showInSymbolList` selectors of the `.tmPreferences` match (the definitions), each through
    /// the `symbolTransformation` that applies there; without such preferences, `entity.name`
    /// scopes (but tags and labels). `indexed` uses `showInIndexedSymbolList` instead, and
    /// `references` `showInIndexedReferenceList` (calls…), which have no default.
    pub fn symbol_regions(&self, id: DocumentId, list: SymbolList) -> Vec<SymbolRegion> {
        let Some(doc) = self.any_document(id) else { return Vec::new() };
        let (key, transformation_key) = match list {
            SymbolList::Symbols => ("showInSymbolList", "symbolTransformation"),
            SymbolList::Indexed => ("showInIndexedSymbolList", "symbolIndexTransformation"),
            SymbolList::References => ("showInIndexedReferenceList", "symbolIndexTransformation"),
        };
        let preferences = &self.state.meta_preferences;
        let mut selector = selectors_where_on(preferences, key);
        if selector.is_empty() && list == SymbolList::Indexed {
            selector = selectors_where_on(preferences, "showInSymbolList");
        }
        if selector.is_empty() && list != SymbolList::References {
            selector = DEFAULT_SYMBOL_SELECTOR.to_string();
        }
        if selector.is_empty() {
            return Vec::new();
        }
        let tokens = self.scoped_ranges(id, 0, usize::MAX);
        let mut symbols: Vec<SymbolRegion> = Vec::new();
        let mut run: Option<(usize, usize, String)> = None;
        let finish = |run: Option<(usize, usize, String)>, symbols: &mut Vec<SymbolRegion>| {
            let Some((start, end, scope)) = run else { return };
            let text: String = doc.text[char_to_byte(&doc.text, start)..char_to_byte(&doc.text, end)].to_string();
            let transformation = meta_value(preferences, &scope, transformation_key)
                .or_else(|| meta_value(preferences, &scope, "symbolTransformation"))
                .and_then(Value::as_str);
            let name = transformation.map_or_else(|| text.trim().to_string(), |t| transform_symbol(&text, t).trim().to_string());
            if !name.is_empty() {
                symbols.push(SymbolRegion { name, start, end, kind: symbol_kind(&scope), scope });
            }
        };
        for token in tokens {
            if !matches_selector(&token.scope, &selector) {
                finish(run.take(), &mut symbols);
                continue;
            }
            // A run stays on its line: a symbol is one line, even when its scope spans several.
            let starts_line = doc.text[..char_to_byte(&doc.text, token.start)].ends_with('\n');
            match &mut run {
                Some((_, end, scope)) if *end == token.start && scope.as_str() == token.scope && !starts_line => *end = token.end,
                _ => {
                    finish(run.take(), &mut symbols);
                    run = Some((token.start, token.end, token.scope));
                }
            }
        }
        finish(run, &mut symbols);
        symbols
    }

    /// `window.symbol_locations(symbol, source, type)`: where a symbol is, in the open files (the
    /// ones that have a path, and all of them for `OpenFiles`). The index is not kept on disk, so
    /// `Index` answers the saved open files too.
    pub fn symbol_locations(&self, symbol: &str, source: SymbolSource, wanted: SymbolType) -> Vec<SymbolLocation> {
        let definitions = wanted != SymbolType::Reference;
        let references = wanted != SymbolType::Definition;
        let mut locations = Vec::new();
        for doc in &self.state.documents {
            if source != SymbolSource::OpenFiles && doc.path.is_none() {
                continue;
            }
            let mut found: Vec<(SymbolRegion, bool)> = Vec::new();
            if definitions {
                found.extend(self.symbol_regions(doc.id, SymbolList::Indexed).into_iter().map(|s| (s, false)));
            }
            if references {
                found.extend(self.symbol_regions(doc.id, SymbolList::References).into_iter().map(|s| (s, true)));
            }
            for (region, reference) in found.into_iter().filter(|(region, _)| region.name == symbol) {
                let (row, column) = line_col(&doc.text, region.start);
                locations.push(SymbolLocation {
                    path: doc.path.clone(),
                    display_name: self.display_path(doc.path.as_deref()),
                    row,
                    column,
                    syntax: doc.syntax.clone(),
                    kind: region.kind.clone(),
                    reference,
                });
            }
        }
        locations
    }

    /// A file's path as the user sees it: relative to the first open folder, else its own path.
    fn display_path(&self, path: Option<&std::path::Path>) -> String {
        let Some(path) = path else { return String::new() };
        self.state
            .folders
            .iter()
            .find_map(|folder| path.strip_prefix(folder).ok())
            .map_or_else(|| path.display().to_string(), |relative| relative.display().to_string())
    }

    pub(crate) fn line_spans(&self, doc: &Document, line: usize) -> Vec<ScopeSpan> {
        self.gateways.syntaxes.line_scopes(doc.id, &doc.text, &doc.syntax, line)
    }
}

/// The zero-based line and column (in chars) of a char point.
fn line_col(text: &str, point: usize) -> (usize, usize) {
    let byte = char_to_byte(text, point);
    let row = text[..byte].matches('\n').count();
    let column = text[..byte].rfind('\n').map_or(byte, |at| byte - at - 1);
    (row, column)
}
