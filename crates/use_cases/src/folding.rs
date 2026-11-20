//! Use cases: code folding (Sublime's `fold`, `unfold`, `unfold_all`, `fold_by_level`, `fold_tag_attributes` and the
//! plugin API's `view.fold` / `unfold`). The folds live in the `Document` (`Document::fold`),
//! the rules of what to fold in `entities::folding`. A caret that lands inside a fold unfolds it,
//! so what it edits is never hidden.

use entities::document::{Document, DocumentId};
use entities::folding::{fold_region_at, folds_at_level, tag_attribute_folds};
use entities::selection::Selection;

use crate::editor::Editor;

impl Editor {
    /// `fold`: each selection, or with an empty one the indented block at the caret.
    pub fn fold_selections(&mut self) {
        let tab_size = self.view_settings().usize("tab_size", 4).max(1);
        let Some(doc) = self.active_document() else {
            return;
        };
        let regions: Vec<(usize, usize)> = doc
            .selections()
            .iter()
            .filter_map(|s| {
                if s.is_empty() {
                    fold_region_at(&doc.text, s.head, tab_size)
                } else {
                    Some((s.start(), s.end()))
                }
            })
            .collect();
        if let Some(doc) = self.active_document_mut() {
            fold_moving_carets(doc, &regions);
        }
    }

    /// A fold button in the gutter was clicked: the folds that start on that line (0-based) open,
    /// or, with none, the block it heads folds.
    pub fn toggle_fold_at_line(&mut self, id: DocumentId, line: usize) {
        let tab_size = self.view_settings().usize("tab_size", 4).max(1);
        let Some(doc) = self.any_document_mut(id) else {
            return;
        };
        let Some((start, end)) = line_chars(&doc.text, line) else {
            return;
        };
        let starting: Vec<(usize, usize)> = doc
            .folds()
            .iter()
            .filter(|&&(a, _)| a >= start && a <= end)
            .map(|&(a, _)| (a, a))
            .collect();
        if !starting.is_empty() {
            doc.unfold(&starting);
        } else if let Some(fold) = fold_region_at(&doc.text, start, tab_size) {
            fold_moving_carets(doc, &[fold]);
        }
    }

    /// `unfold`: the folds the selections touch (with an empty one, those on the caret's line).
    pub fn unfold_selections(&mut self) {
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let regions: Vec<(usize, usize)> = doc
            .selections()
            .iter()
            .map(|s| {
                if s.is_empty() {
                    line_span(&doc.text, s.head)
                } else {
                    (s.start(), s.end())
                }
            })
            .collect();
        doc.unfold(&regions);
    }

    /// `unfold_all`.
    pub fn unfold_all(&mut self) {
        if let Some(doc) = self.active_document_mut() {
            doc.unfold_all();
        }
    }

    /// `fold_by_level {"level"}`: every block at that indentation level (1 = the outermost).
    pub fn fold_by_level(&mut self, level: usize) {
        let tab_size = self.view_settings().usize("tab_size", 4).max(1);
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let folds = folds_at_level(&doc.text, level.max(1), tab_size);
        doc.unfold_all();
        doc.fold(&folds);
        let head = doc.selection().head;
        if let Some(&(a, _)) = doc.folds().iter().find(|&&(a, b)| head > a && head <= b) {
            doc.set_selection(Selection::caret(a));
        }
    }

    /// `fold_tag_attributes`: the attributes of every tag (HTML, XML: found by their scopes).
    pub fn fold_tag_attributes(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let tokens = self.line_tokens(doc, 0..usize::MAX);
        let pieces: Vec<(usize, usize, &str)> = tokens
            .iter()
            .map(|t| (t.start, t.end, t.scope.as_str()))
            .collect();
        let folds = tag_attribute_folds(&doc.text, &pieces);
        if let Some(doc) = self.active_document_mut() {
            fold_moving_carets(doc, &folds);
        }
    }

    /// The view's "…" marker of a fold was clicked: that fold opens.
    pub fn unfold_at(&mut self, id: DocumentId, point: usize) {
        if let Some(doc) = self.any_document_mut(id) {
            doc.unfold(&[(point, point)]);
        }
    }

    /// A caret inside a fold (moved there by a click, the keys or a command) unfolds it.
    pub(crate) fn unfold_around_carets(&mut self, id: DocumentId) {
        let Some(doc) = self.any_document_mut(id) else {
            return;
        };
        if doc.folds().is_empty() {
            return;
        }
        let inside: Vec<(usize, usize)> = doc
            .selections()
            .iter()
            .flat_map(|s| [s.anchor, s.head])
            .filter(|&p| doc.folds().iter().any(|&(a, b)| p > a && p <= b))
            .map(|p| (p, p))
            .collect();
        if !inside.is_empty() {
            doc.unfold(&inside);
        }
    }
}

/// The chars of the line at `point`, without its line break (the last line past the end).
fn line_span(text: &str, point: usize) -> (usize, usize) {
    let mut start = 0;
    let mut span = (0, 0);
    for line in text.split('\n') {
        span = (start, start + line.chars().count());
        if point <= span.1 {
            break;
        }
        start = span.1 + 1;
    }
    span
}

/// Folds `regions`; the carets inside go before what they folded.
fn fold_moving_carets(doc: &mut Document, regions: &[(usize, usize)]) {
    doc.fold(regions);
    let moved = |s: &Selection| {
        let inside = regions.iter().find(|&&(a, b)| s.head > a && s.head <= b);
        inside.map_or(*s, |&(a, _)| Selection::caret(a))
    };
    let primary = moved(&doc.selection());
    let extra: Vec<Selection> = doc.extra_selections().iter().map(moved).collect();
    doc.set_selections(primary, extra);
}

/// The chars of line `line` (0-based), without its line break; `None` past the last line.
fn line_chars(text: &str, line: usize) -> Option<(usize, usize)> {
    let mut start = 0;
    for (index, content) in text.split('\n').enumerate() {
        let end = start + content.chars().count();
        if index == line {
            return Some((start, end));
        }
        start = end + 1;
    }
    None
}
