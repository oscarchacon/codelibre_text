//! Use cases: find and replace in the active document.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use entities::document::DocumentId;
use entities::find::{self, FindOptions};
use entities::text::{selected_text, word_at};

use crate::editor::Editor;

/// Matches of the last search, reused while the text and the options do not change.
pub struct FindCache {
    options: FindOptions,
    document: DocumentId,
    text_hash: u64,
    matches: Vec<Range<usize>>,
}

impl Editor {
    pub fn set_find_options(&mut self, options: FindOptions) {
        self.state.find = options;
    }

    pub fn set_replacement(&mut self, replacement: String) {
        self.state.replacement = replacement;
    }

    /// Opening the find panel uses a single-line selection as the query, like Sublime.
    pub fn use_selection_as_query_if_single_line(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let selected = selected_text(&doc.text, doc.selection());
        if !selected.is_empty() && !selected.contains('\n') {
            self.state.find.query = selected.to_string();
        }
    }

    /// `slurp_find_string` / `find_under`: the selection (or the word under the caret) becomes the
    /// query. With `whole_word_if_empty`, a word taken from the caret is searched as a whole word.
    pub fn use_selection_as_query(&mut self, whole_word_if_empty: bool) {
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let mut selection = doc.selection();
        let from_caret = selection.is_empty();
        if from_caret {
            selection = word_at(&doc.text, selection.head);
            doc.set_selection(selection);
        }
        let text = selected_text(&doc.text, selection).to_string();
        if text.is_empty() {
            return;
        }
        let find = &mut self.state.find;
        find.query = text;
        find.regex = false;
        find.whole_word = from_caret && whole_word_if_empty;
    }

    /// Selects the next (or previous) match. Returns false when the query is empty.
    pub fn find_next(&mut self, forward: bool) -> bool {
        let options = self.state.find.clone();
        let pattern = match options.pattern() {
            Ok(Some(pattern)) => pattern,
            Ok(None) => return false,
            Err(e) => {
                self.state.find_error = Some(e);
                return true;
            }
        };
        let Some(doc) = self.active_document_mut() else {
            return true;
        };
        match find::find_from(&pattern, &doc.text, doc.selection(), forward, options.wrap) {
            Some(selection) => doc.set_selection(selection),
            None => self.notify(format!("Unable to find {}", options.query)),
        }
        true
    }

    /// `find_under_expand` (Ctrl+D): selects the word under the caret, then the next occurrence.
    /// (Single selection: the next occurrence replaces the selection instead of adding a cursor.)
    pub fn select_word_or_next_occurrence(&mut self) {
        let selection_empty = self.active_document().is_some_and(|d| d.selection().is_empty());
        if selection_empty {
            self.run_text_command(crate::editing::TextCommand::SelectWord);
        } else {
            self.use_selection_as_query(false);
            self.find_next(true);
        }
    }

    pub fn replace_next(&mut self) {
        let options = self.state.find.clone();
        let replacement = self.state.replacement.clone();
        let Ok(Some(pattern)) = options.pattern() else {
            return;
        };
        if let Some(doc) = self.active_document_mut() {
            doc.edit(|text, sel| Some(find::replace_next(&pattern, text, sel, &replacement, &options)));
        }
    }

    pub fn replace_all(&mut self) {
        let options = self.state.find.clone();
        let replacement = self.state.replacement.clone();
        let Ok(Some(pattern)) = options.pattern() else {
            return;
        };
        let mut count = 0;
        if let Some(doc) = self.active_document_mut() {
            doc.edit(|text, sel| {
                count = find::replace_all(&pattern, text, &replacement, &options);
                Some(sel.clamped(text.chars().count()))
            });
        }
        self.notify(format!("Replaced {count} occurrences"));
    }

    /// Byte ranges of every match in the active document (cached between calls).
    pub fn find_matches(&mut self) -> Vec<Range<usize>> {
        if self.state.find.query.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.active_document() else {
            return Vec::new();
        };
        let mut hasher = DefaultHasher::new();
        doc.text.hash(&mut hasher);
        let text_hash = hasher.finish();
        if let Some(cache) = &self.state.find_cache {
            if cache.options == self.state.find && cache.document == doc.id && cache.text_hash == text_hash {
                return cache.matches.clone();
            }
        }
        let (matches, error) = match self.state.find.pattern() {
            Ok(Some(pattern)) => (find::find_all(&pattern, &doc.text), None),
            Ok(None) => (Vec::new(), None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let document = doc.id;
        self.state.find_error = error;
        self.state.find_cache = Some(FindCache {
            options: self.state.find.clone(),
            document,
            text_hash,
            matches: matches.clone(),
        });
        matches
    }
}
