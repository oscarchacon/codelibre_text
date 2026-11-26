//! Use cases: find and replace in the active document.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use entities::document::{Document, DocumentId, RegionSet};
use entities::find::{self, FindOptions, FindToggle, Regex};
use entities::selection::Selection;
use entities::text::{char_to_byte, selected_text, view_word_at, word_at};

use crate::editor::Editor;

/// What makes Sublime record a command in a view's history: its text, selections and regions.
#[derive(Debug, Clone, PartialEq)]
pub struct OccurrenceView {
    change_count: u64,
    selections: Vec<Selection>,
    regions: Vec<RegionSet>,
}

impl OccurrenceView {
    fn of(doc: &Document) -> Self {
        Self {
            change_count: doc.change_count(),
            selections: doc.selections(),
            regions: doc.region_sets().to_vec(),
        }
    }
}

/// The hidden region set that keeps Find "In selection"'s area in the view it was taken in: it moves
/// with the text, and is drawn (outlined) while the find panel shows.
pub const FIND_AREA: &str = "codelibre_find_in_selection";

/// `sublime.DRAW_NO_FILL`: the area is outlined.
const DRAW_NO_FILL: u64 = 32;

/// Matches of the last search, reused while the text and the options do not change.
pub struct FindCache {
    options: FindOptions,
    document: DocumentId,
    text_hash: u64,
    area: Option<Vec<Range<usize>>>,
    matches: Vec<Range<usize>>,
}

impl Editor {
    pub fn set_find_options(&mut self, options: FindOptions) {
        let turned = options.in_selection != self.state.find.in_selection;
        self.state.find = options;
        if turned {
            self.take_find_area();
        }
    }

    pub fn set_replacement(&mut self, replacement: String) {
        self.state.replacement = replacement;
    }

    /// `toggle_regex`, `toggle_case_sensitive`, `toggle_whole_word`, `toggle_preserve_case`,
    /// `toggle_wrap`, `toggle_in_selection`, `toggle_highlight`: they work with the panel hidden
    /// too, as in Sublime.
    pub fn toggle_find_option(&mut self, option: FindToggle) {
        self.state.find.toggle(option);
        if option == FindToggle::InSelection {
            self.take_find_area();
        }
    }

    /// "In selection" turned on keeps the active view's selections as the area searched; off
    /// forgets it. Carets alone keep none: the whole text is searched then.
    fn take_find_area(&mut self) {
        for doc in &mut self.state.documents {
            doc.erase_regions(FIND_AREA);
        }
        if !self.state.find.in_selection {
            return;
        }
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let regions: Vec<(usize, usize)> = doc
            .selections()
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| (s.start(), s.end()))
            .collect();
        if !regions.is_empty() {
            doc.add_regions(RegionSet {
                key: FIND_AREA.to_string(),
                regions,
                scope: "region.bluish".to_string(),
                flags: DRAW_NO_FILL,
                ..RegionSet::default()
            });
        }
    }

    /// The byte ranges "In selection" searches in `doc`, or `None` for the whole text.
    fn find_area(&self, doc: &Document) -> Option<Vec<Range<usize>>> {
        if !self.state.find.in_selection {
            return None;
        }
        let area: Vec<Range<usize>> = doc
            .regions(FIND_AREA)
            .into_iter()
            .map(|(a, b)| char_to_byte(&doc.text, a.min(b))..char_to_byte(&doc.text, a.max(b)))
            .filter(|r| !r.is_empty())
            .collect();
        (!area.is_empty()).then_some(area)
    }

    /// The matches of `pattern` in `doc`, inside the area with "In selection".
    fn matches_in(&self, doc: &Document, pattern: &Regex) -> Vec<Range<usize>> {
        let matches = find::find_all(pattern, &doc.text);
        match self.find_area(doc) {
            Some(area) => find::inside(matches, &area),
            None => matches,
        }
    }

    /// `slurp_find_string`: the selection (or the word under the caret, then selected) becomes
    /// the query. As Sublime 4215 does, the other options stay; with regex on the text is escaped.
    pub fn slurp_find_string(&mut self) {
        let Some(text) = self.slurp_selection(true) else {
            return;
        };
        let find = &mut self.state.find;
        find.query = if find.regex {
            find::escape(&text)
        } else {
            text
        };
    }

    /// `slurp_replace_string`: the selection (or the word under the caret) becomes the
    /// replacement, as written (Sublime 4215 escapes nothing, even with regex on).
    pub fn slurp_replace_string(&mut self) {
        if let Some(text) = self.slurp_selection(false) {
            self.state.replacement = text;
        }
    }

    /// The selected text, or the word at the caret (`select_word`: also selected); `None` when
    /// it is empty.
    fn slurp_selection(&mut self, select_word: bool) -> Option<String> {
        let doc = self.active_document_mut()?;
        let mut selection = doc.selection();
        if selection.is_empty() {
            selection = word_at(&doc.text, selection.head);
            if select_word {
                doc.set_selection(selection);
            }
        }
        let text = selected_text(&doc.text, selection);
        (!text.is_empty()).then(|| text.to_string())
    }

    /// Opening the find panel over a selection, as Sublime 4215 does: a single-line one becomes
    /// the query (escaped with regex on). With several lines selected and
    /// `auto_find_in_selection` (or "In selection" already on), the find and replace panels
    /// search in them instead (`in_files`: Find in Files, which has no such option).
    pub fn use_selection_for_find_panel(&mut self, in_files: bool) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let selected = selected_text(&doc.text, doc.selection()).to_string();
        let multi_line = doc
            .selections()
            .iter()
            .any(|s| selected_text(&doc.text, *s).contains('\n'));
        if multi_line {
            let auto = self.view_settings().bool("auto_find_in_selection", false);
            if !in_files && (auto || self.state.find.in_selection) {
                self.state.find.in_selection = true;
                self.take_find_area();
            }
            return;
        }
        if !selected.is_empty() {
            let find = &mut self.state.find;
            find.query = if find.regex {
                find::escape(&selected)
            } else {
                selected
            };
        }
    }

    /// `find_under` / `find_under_prev`: the selection (or the word under the caret) becomes the
    /// query, literal; a word taken from the caret is searched as a whole word.
    pub fn use_selection_as_query(&mut self) {
        let from_caret = self
            .active_document()
            .is_some_and(|doc| doc.selection().is_empty());
        let Some(text) = self.slurp_selection(true) else {
            return;
        };
        let find = &mut self.state.find;
        find.query = text;
        find.regex = false;
        find.whole_word = from_caret;
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
        if !self.select_match_after(&pattern, forward) {
            self.notify(self.tr_args("Unable to find {query}", &[("query", &options.query)]));
        }
        true
    }

    /// Selects the match after (or before) the selection, in the area with "In selection".
    /// Returns false when there is none.
    fn select_match_after(&mut self, pattern: &Regex, forward: bool) -> bool {
        let Some(doc) = self.active_document() else {
            return true;
        };
        let matches = self.matches_in(doc, pattern);
        let wrap = self.state.find.wrap;
        let found = find::find_among(&matches, &doc.text, doc.selection(), forward, wrap);
        match (found, self.active_document_mut()) {
            (Some(selection), Some(doc)) => {
                doc.set_selection(selection);
                true
            }
            (None, _) => false,
            (Some(_), None) => true,
        }
    }

    /// `find_under_expand` (Ctrl+D), as Sublime 4215 does it: with carets, each caret selects its
    /// word (`view_word_at`), nothing is added, and the view's occurrences are whole words from
    /// then on (see [`Self::note_command_for_occurrences`]); else the next occurrence of the last
    /// selection's text (by position, not the newest) after it, wrapping around, is added as the
    /// primary selection. The search is case sensitive.
    pub fn select_word_or_next_occurrence(&mut self) {
        let Some(id) = self.active_document().map(|d| d.id) else {
            return;
        };
        if self.select_words_at_carets(id) {
            return;
        }
        let whole_word = self.state.whole_word_occurrences.contains(&id);
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let selections = doc.selections();
        let Some(&last) = selections.last() else {
            return;
        };
        let Ok(Some(pattern)) = occurrence_options(&doc.text, last, whole_word).pattern() else {
            return;
        };
        if let Some(next) = find::next_unselected(&pattern, &doc.text, last, &selections) {
            doc.add_selection(next);
        }
    }

    /// `find_under_expand_skip` (Ctrl+K Ctrl+D), as Sublime 4215 does it: carets select their
    /// words first (as Ctrl+D's do); then the last selection (by position) gives way to the next
    /// occurrence of its text that is not selected, wrapping around. When there is none it is only
    /// taken away, unless it is the only selection.
    pub fn skip_to_next_occurrence(&mut self) {
        let Some(id) = self.active_document().map(|d| d.id) else {
            return;
        };
        self.select_words_at_carets(id);
        let whole_word = self.state.whole_word_occurrences.contains(&id);
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let mut others = doc.selections();
        let Some(last) = others.pop() else {
            return;
        };
        let Ok(Some(pattern)) = occurrence_options(&doc.text, last, whole_word).pattern() else {
            return;
        };
        let all = doc.selections();
        match find::next_unselected(&pattern, &doc.text, last, &all) {
            Some(next) => doc.set_selections(next, others),
            None => {
                if let Some(primary) = others.pop() {
                    doc.set_selections(primary, others);
                }
            }
        }
    }

    /// Ctrl+D's first step: every caret selects its word (`view_word_at`), and the view's
    /// occurrences are whole words from then on. True when there was a caret.
    fn select_words_at_carets(&mut self, id: DocumentId) -> bool {
        let Some(doc) = self.active_document_mut() else {
            return false;
        };
        let primary = doc.selection();
        let extra = doc.extra_selections().to_vec();
        if !primary.is_empty() && extra.iter().all(|s| !s.is_empty()) {
            return false;
        }
        let text = &doc.text;
        let word = |s: Selection| {
            if s.is_empty() {
                view_word_at(text, s.head)
            } else {
                s
            }
        };
        let primary = word(primary);
        let extra: Vec<Selection> = extra.into_iter().map(word).collect();
        doc.set_selections(primary, extra);
        if !self.state.whole_word_occurrences.contains(&id) {
            self.state.whole_word_occurrences.push(id);
        }
        true
    }

    /// A command is about to run. The whole-word occurrences Ctrl+D started from carets last, in
    /// Sublime 4215, while the last command in a view's history is `find_under_expand` or
    /// `find_under_expand_skip`, and it records a command that changed the view's text,
    /// selections or regions (setting the same selections is no change), `copy` and `insert`
    /// always. So `copy` and `insert` end the active view's here; a command run with
    /// [`Self::occurrence_views`] noted before and [`Self::end_changed_occurrences`] after ends
    /// those of the views it changed (window and application commands change none unless they
    /// run a text command); what the text widget does itself ends them where it changes the text
    /// or the caret.
    pub fn note_command_for_occurrences(&mut self, name: &str) {
        const ALWAYS_RECORDED: &[&str] = &["copy", "insert"];
        if ALWAYS_RECORDED.contains(&name) {
            if let Some(id) = self.active_document().map(|d| d.id) {
                self.end_whole_word_occurrences(id);
            }
        }
    }

    /// How the views with whole-word occurrences are before the command `name` runs: none for
    /// Ctrl+D and Ctrl+K Ctrl+D, which keep them whatever they change.
    pub fn occurrence_views(&self, name: &str) -> Vec<(DocumentId, OccurrenceView)> {
        if matches!(name, "find_under_expand" | "find_under_expand_skip") {
            return Vec::new();
        }
        self.state
            .whole_word_occurrences
            .iter()
            .filter_map(|&id| Some((id, OccurrenceView::of(self.any_document(id)?))))
            .collect()
    }

    /// Ends the whole-word occurrences of the views that changed since `before`.
    pub fn end_changed_occurrences(&mut self, before: Vec<(DocumentId, OccurrenceView)>) {
        for (id, before) in before {
            let after = self.any_document(id).map(OccurrenceView::of);
            if after.as_ref() != Some(&before) {
                self.end_whole_word_occurrences(id);
            }
        }
    }

    /// Does what the text widget does by itself at every selection (typing, deleting, moving the
    /// carets): a command Sublime records when it changes the active view, which then loses its
    /// whole-word occurrences.
    pub(crate) fn as_widget_command(&mut self, edit: impl FnOnce(&mut Self)) {
        let before = self
            .active_document()
            .map(|d| (d.id, OccurrenceView::of(d)));
        edit(self);
        if let Some((id, before)) = before {
            if self.any_document(id).map(OccurrenceView::of).as_ref() != Some(&before) {
                self.end_whole_word_occurrences(id);
            }
        }
    }

    /// Ctrl+D's next occurrences in this view are found by a plain search again.
    pub fn end_whole_word_occurrences(&mut self, id: DocumentId) {
        self.state.whole_word_occurrences.retain(|&view| view != id);
    }

    /// `find_all_under` (Alt+F3), as Sublime 4215 does it: every occurrence of the primary
    /// selection, or, from a caret, of its word (`view_word_at`) as a whole word (so `+=` under
    /// the caret finds nothing and the caret stays). Case sensitive.
    pub fn select_all_occurrences(&mut self) {
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let mut selection = doc.selection();
        let from_caret = selection.is_empty();
        if from_caret {
            selection = view_word_at(&doc.text, selection.head);
        }
        let Ok(Some(pattern)) = occurrence_options(&doc.text, selection, from_caret).pattern()
        else {
            return;
        };
        let matches = find::find_all_selections(&pattern, &doc.text);
        if matches.is_empty() {
            return;
        }
        // The primary selection stays where it was, so the view does not jump.
        let primary = matches
            .iter()
            .copied()
            .find(|m| m.start() == selection.start() && m.end() == selection.end())
            .unwrap_or(selection);
        doc.set_selections(primary, matches);
    }

    /// `find_all` in the find panel: selects every match of the query in the active view (in the
    /// area with "In selection").
    pub fn select_find_matches(&mut self) {
        let Ok(Some(pattern)) = self.state.find.pattern() else {
            return;
        };
        let Some(doc) = self.active_document() else {
            return;
        };
        let matches = find::to_selections(&doc.text, &self.matches_in(doc, &pattern));
        match (matches.first(), self.active_document_mut()) {
            (Some(&first), Some(doc)) => doc.set_selections(first, matches),
            _ => {
                let query = self.state.find.query.clone();
                self.notify(self.tr_args("Unable to find {query}", &[("query", &query)]));
            }
        }
    }

    /// Replaces the selection if it is a match, then selects the next match (in the area with
    /// "In selection").
    pub fn replace_next(&mut self) {
        let options = self.state.find.clone();
        let replacement = self.state.replacement.clone();
        let Ok(Some(pattern)) = options.pattern() else {
            return;
        };
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        doc.edit(|text, sel| find::replace_selected(&pattern, text, sel, &replacement, &options));
        self.select_match_after(&pattern, true);
    }

    /// Replaces every match (in the area with "In selection", one part at a time so the area
    /// keeps its ends).
    pub fn replace_all(&mut self) {
        let options = self.state.find.clone();
        let replacement = self.state.replacement.clone();
        let Ok(Some(pattern)) = options.pattern() else {
            return;
        };
        let Some(doc) = self.active_document() else {
            return;
        };
        let mut parts = self
            .find_area(doc)
            .unwrap_or_else(|| std::iter::once(0..doc.text.len()).collect());
        let mut count = 0;
        if let Some(doc) = self.active_document_mut() {
            // From the last part, so the earlier ones keep their bytes.
            parts.sort_by_key(|r| std::cmp::Reverse(r.start));
            for part in parts {
                doc.edit(|text, sel| {
                    let replaced =
                        find::replace_all_in(&pattern, text, &replacement, &options, part);
                    count += replaced;
                    (replaced > 0).then(|| sel.clamped(text.chars().count()))
                });
            }
        }
        self.notify(self.tr_n(
            "Replaced {count} occurrence",
            "Replaced {count} occurrences",
            count,
            &[],
        ));
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
        let area = self.find_area(doc);
        if let Some(cache) = &self.state.find_cache {
            if cache.options == self.state.find
                && cache.document == doc.id
                && cache.text_hash == text_hash
                && cache.area == area
            {
                return cache.matches.clone();
            }
        }
        let (matches, error) = match self.state.find.pattern() {
            Ok(Some(pattern)) => (self.matches_in(doc, &pattern), None),
            Ok(None) => (Vec::new(), None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let document = doc.id;
        self.state.find_error = error;
        self.state.find_cache = Some(FindCache {
            options: self.state.find.clone(),
            document,
            text_hash,
            area,
            matches: matches.clone(),
        });
        matches
    }
}

/// How `find_under_expand` and `find_all_under` look for the selected text: case sensitive, and
/// whole words (`\b`, so a selection that starts or ends with no word character matches
/// nothing) only when the selection came from a caret.
fn occurrence_options(text: &str, selection: Selection, whole_word: bool) -> FindOptions {
    FindOptions {
        query: selected_text(text, selection).to_string(),
        case_sensitive: true,
        whole_word,
        ..FindOptions::default()
    }
}
