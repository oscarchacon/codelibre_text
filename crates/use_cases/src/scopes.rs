//! Use cases: the scopes of the text (from the syntax gateway), for plugins (`view.scope_name`,
//! `extract_scope`, `find_by_selector`…) and the `selector` key binding context. Points are chars.

use entities::document::{Document, DocumentId};
use entities::selector::matches_selector;
use entities::text::char_to_byte;

use crate::editor::Editor;
use crate::gateways::ScopeSpan;

/// A piece of the text with one scope, in chars of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedRange {
    pub start: usize,
    pub end: usize,
    pub scope: String,
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
        let Some(doc) = self.active_document() else { return Vec::new() };
        doc.selections().iter().filter_map(|s| self.scope_name(doc.id, s.head)).collect()
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
    pub fn extract_scope(&self, id: DocumentId, point: usize) -> Option<(usize, usize)> {
        let scope = self.scope_name(id, point)?;
        let tokens = self.scoped_ranges(id, 0, usize::MAX);
        let at = tokens.iter().position(|t| t.start <= point && point < t.end).or(tokens.len().checked_sub(1))?;
        let same = |t: &ScopedRange| t.scope == scope.trim_end() || t.scope.starts_with(scope.as_str());
        let first = tokens[..at].iter().rposition(|t| !same(t)).map_or(0, |i| i + 1);
        let last = tokens[at..].iter().position(|t| !same(t)).map_or(tokens.len(), |i| at + i);
        Some(if first < last { (tokens[first].start, tokens[last - 1].end) } else { (point, point) })
    }

    /// The pieces of the text between `start` and `end` (chars) and their scopes, in order.
    pub fn scoped_ranges(&self, id: DocumentId, start: usize, end: usize) -> Vec<ScopedRange> {
        let Some(doc) = self.any_document(id) else { return Vec::new() };
        let first_line = doc.text[..char_to_byte(&doc.text, start)].matches('\n').count();
        let (mut line_char, mut tokens) = (0, Vec::new());
        for (line, line_text) in doc.text.split_inclusive('\n').enumerate() {
            if line_char >= end {
                break;
            }
            if line >= first_line {
                for span in self.line_spans(doc, line) {
                    let text = &line_text[..span.range.end];
                    let a = line_char + line_text[..span.range.start].chars().count();
                    let b = line_char + text.chars().count();
                    if b > start && a < end {
                        tokens.push(ScopedRange { start: a.max(start), end: b.min(end), scope: span.scope });
                    }
                }
            }
            line_char += line_text.chars().count();
        }
        tokens
    }

    fn line_spans(&self, doc: &Document, line: usize) -> Vec<ScopeSpan> {
        self.gateways.syntaxes.line_scopes(doc.id, &doc.text, &doc.syntax, line)
    }
}
