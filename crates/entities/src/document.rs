//! An open buffer: its text, the file it belongs to, how that file is encoded, and the selection.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::selection::Selection;
use crate::text::char_to_byte;

pub type DocumentId = u64;

/// Name of a new document with no text yet (the interface shows it translated).
pub const UNTITLED: &str = "untitled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Unix,
    Windows,
}

impl LineEnding {
    /// Parses Sublime's `default_line_ending` values (`"system"`, `"windows"`, `"unix"`).
    pub fn from_setting(value: Option<&str>, system: LineEnding) -> Self {
        match value {
            Some("windows") => LineEnding::Windows,
            Some("unix") | Some("mac") => LineEnding::Unix,
            _ => system,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Unix => "Unix",
            LineEnding::Windows => "Windows",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8WithBom,
    /// Files that are not valid UTF-8 are read as ISO-8859-1.
    Latin1,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8WithBom => "UTF-8 with BOM",
            Encoding::Latin1 => "Western (ISO 8859-1)",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DocumentError {
    Binary,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    /// Name for buffers that are not files (e.g. the read-only default settings).
    pub scratch_name: Option<String>,
    /// Never dirty, so closing it does not ask to save (a plugin's `view.set_scratch(True)`).
    pub scratch: bool,
    pub read_only: bool,
    /// The text, always with `\n` line endings in memory.
    pub text: String,
    saved_text: String,
    pub line_ending: LineEnding,
    saved_line_ending: LineEnding,
    pub encoding: Encoding,
    /// Name of the syntax used to highlight it (e.g. `Rust`, `Plain Text`).
    pub syntax: String,
    /// When the file was last read or written; used to notice changes made by other programs.
    pub modified_at: Option<SystemTime>,
    /// The group of the window (a pane of the layout) whose tabs it is in.
    pub group: usize,
    /// A preview (Sublime's transient view: a file clicked once in the side bar, `TRANSIENT`):
    /// the next preview in its group takes its place, until it is edited or kept.
    pub transient: bool,
    /// The buffer it shows: its own id, or that of the view it is a clone of (`clone_file`),
    /// whose text it shares (see [`Document::follow_buffer`]).
    pub buffer: DocumentId,
    /// The primary selection: the one the text widget shows and edits.
    selection: Selection,
    /// More selections added by `find_under_expand` (Ctrl+D), `select_lines`, `find_all_under`…
    /// Any other selection change drops them.
    extra_selections: Vec<Selection>,
    /// Increases whenever the selection is changed by a command (not by the user typing),
    /// so the view knows it has to move its caret.
    selection_revision: u64,
    /// Increases with every change to the text (Sublime's `view.change_count()`).
    change_count: u64,
    /// Regions plugins mark (`view.add_regions`), in the order they were added.
    region_sets: Vec<RegionSet>,
    /// HTML plugins show in the text (`view.add_phantom`), in the order they were added.
    phantoms: Vec<Phantom>,
    /// Where a plugin asked the view to scroll (`view.show`), and how many times it asked.
    show_request: Option<ShowRequest>,
    show_revision: u64,
    /// A copy of the text, kept only while something needs to know what each edit changed (regions,
    /// a change log): the widget edits `text` in place, so comparing is the way to find out.
    tracked_text: Option<String>,
    /// The changes not taken yet, when a plugin listens to them.
    change_log: Option<Vec<TextChange>>,
    /// The chars on screen, as the view last reported them.
    visible_range: Option<(usize, usize)>,
    /// The fields of the snippet last inserted, while Tab / Shift+Tab visit them.
    snippet: Option<SnippetFields>,
}

/// A snippet's fields (see `text_commands::insert_snippet_fields`): each field's regions, in
/// visiting order (the last is where the caret ends), and the field the selection is in. They
/// move with the text, and a field grows with what is typed at its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetFields {
    pub fields: Vec<Vec<(usize, usize)>>,
    pub current: usize,
}

/// One edit of the text, with positions as they were before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// Chars `start..end` were replaced by `text`.
    pub start: usize,
    pub end: usize,
    /// Zero-based line and column of `start` and `end`.
    pub start_position: (usize, usize),
    pub end_position: (usize, usize),
    pub text: String,
}

/// `sublime.PERSISTENT`: a region set kept in the session, so it survives a restart.
pub const PERSISTENT: u64 = 16;

/// A named set of regions a plugin marks (a linter's errors, a search's matches…). The regions
/// move with the text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionSet {
    pub key: String,
    /// `(a, b)` char positions, like a selection's anchor and head.
    pub regions: Vec<(usize, usize)>,
    /// Gives the color through the color scheme (e.g. `region.redish`, `invalid`).
    pub scope: String,
    /// Gutter icon: `dot`, `circle`, `bookmark` or a resource path; empty for none.
    pub icon: String,
    /// Sublime's drawing flags (`sublime.DRAW_NO_FILL`, `DRAW_SQUIGGLY_UNDERLINE`, `HIDDEN`…).
    pub flags: u64,
    /// minihtml shown at the right of each region's line (`annotations`), one per region (a
    /// missing one: none), in the order the regions were given.
    pub annotations: Vec<String>,
    /// The annotations' color (`annotation_color`, `#rrggbb` or a CSS color); empty: the scope's.
    pub annotation_color: String,
}

/// HTML a plugin shows in the text (`view.add_phantom`): an inline note, a block below a line…
/// Its region moves with the text, like a [`RegionSet`]'s.
#[derive(Debug, Clone, PartialEq)]
pub struct Phantom {
    /// Given by the editor; plugins erase and query it by this.
    pub id: u64,
    /// Phantoms with the same key are erased together (`erase_phantoms`).
    pub key: String,
    /// `(a, b)` char positions.
    pub region: (usize, usize),
    /// minihtml (see [`crate::minihtml`]).
    pub content: String,
    /// Sublime's `LAYOUT_INLINE` (0), `LAYOUT_BELOW` (1) or `LAYOUT_BLOCK` (2).
    pub layout: u64,
}

/// A plugin's `view.show(point)` / `view.show_at_center(point)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowRequest {
    pub point: usize,
    pub center: bool,
}

impl Document {
    pub fn new_untitled(id: DocumentId, line_ending: LineEnding) -> Self {
        Self {
            id,
            path: None,
            scratch_name: None,
            scratch: false,
            read_only: false,
            text: String::new(),
            saved_text: String::new(),
            line_ending,
            saved_line_ending: line_ending,
            encoding: Encoding::Utf8,
            syntax: "Plain Text".into(),
            modified_at: None,
            group: 0,
            transient: false,
            buffer: id,
            selection: Selection::caret(0),
            extra_selections: Vec::new(),
            selection_revision: 1,
            change_count: 0,
            region_sets: Vec::new(),
            phantoms: Vec::new(),
            show_request: None,
            show_revision: 0,
            tracked_text: None,
            change_log: None,
            visible_range: None,
            snippet: None,
        }
    }

    /// A buffer that does not belong to a file.
    pub fn scratch(id: DocumentId, name: &str, text: &str, syntax: &str, read_only: bool) -> Self {
        let mut doc = Self::new_untitled(id, LineEnding::Unix);
        doc.scratch_name = Some(name.to_string());
        doc.text = text.to_string();
        doc.saved_text = text.to_string();
        doc.syntax = syntax.to_string();
        doc.read_only = read_only;
        doc
    }

    /// A document for a file's contents: detects the encoding and line endings.
    pub fn from_bytes(id: DocumentId, path: &Path, bytes: Vec<u8>, modified_at: Option<SystemTime>) -> Result<Self, DocumentError> {
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Err(DocumentError::Binary);
        }
        let (text, encoding) = decode(bytes);
        let line_ending = if text.contains("\r\n") {
            LineEnding::Windows
        } else {
            LineEnding::Unix
        };
        let text = text.replace("\r\n", "\n");
        let mut doc = Self::new_untitled(id, line_ending);
        doc.saved_text = text.clone();
        doc.text = text;
        doc.encoding = encoding;
        doc.path = Some(path.to_path_buf());
        doc.modified_at = modified_at;
        Ok(doc)
    }

    /// The bytes to write to disk, with the original line endings and encoding.
    /// Latin-1 files that now contain other characters become UTF-8.
    pub fn to_bytes(&mut self) -> Vec<u8> {
        let text = match self.line_ending {
            LineEnding::Windows => self.text.replace('\n', "\r\n"),
            LineEnding::Unix => self.text.clone(),
        };
        match self.encoding {
            Encoding::Utf8 => text.into_bytes(),
            Encoding::Utf8WithBom => {
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend(text.into_bytes());
                bytes
            }
            Encoding::Latin1 if text.chars().all(|c| (c as u32) < 256) => text.chars().map(|c| c as u8).collect(),
            Encoding::Latin1 => {
                self.encoding = Encoding::Utf8;
                text.into_bytes()
            }
        }
    }

    /// Another view of the same buffer (`clone_file`): the text, file and selection, not what
    /// belongs to a view (regions, phantoms, a snippet's fields, the scroll).
    pub fn clone_view(&self, id: DocumentId) -> Document {
        let mut view = Document::new_untitled(id, self.line_ending);
        view.buffer = self.buffer;
        view.path = self.path.clone();
        view.scratch_name = self.scratch_name.clone();
        view.scratch = self.scratch;
        view.read_only = self.read_only;
        view.text = self.text.clone();
        view.saved_text = self.saved_text.clone();
        view.saved_line_ending = self.saved_line_ending;
        view.encoding = self.encoding;
        view.syntax = self.syntax.clone();
        view.modified_at = self.modified_at;
        view.group = self.group;
        view.change_count = self.change_count;
        view.set_selections(self.selection, self.extra_selections.clone());
        view
    }

    /// Takes the buffer of a view of it that was edited or saved (`source`): its text (the
    /// selections and regions move over the change as over an edit), file, saved text and
    /// settings. Its own change count goes up by one change.
    pub fn follow_buffer(&mut self, source: &Document) {
        if self.text != source.text {
            let before = std::mem::replace(&mut self.text, source.text.clone());
            let prefix = common_prefix(&before, &self.text);
            let suffix = common_suffix(&before[prefix..], &self.text[prefix..]);
            let end_byte = before.len() - suffix;
            let inserted_text = self.text[prefix..self.text.len() - suffix].to_string();
            let (start, end) = (before[..prefix].chars().count(), before[..end_byte].chars().count());
            let inserted = inserted_text.chars().count();
            let map = |p: usize| moved_position(p, start, end, inserted);
            let moved = |s: &Selection| Selection::new(map(s.anchor), map(s.head));
            let primary = moved(&self.selection);
            let extra: Vec<Selection> = self.extra_selections.iter().map(moved).collect();
            if self.tracked_text.is_some() {
                // The regions of this view move over the change.
                self.tracked_text = Some(before);
                self.record_change(prefix, end_byte, &inserted_text);
            }
            self.change_count += 1;
            self.selection = primary.clamped(self.char_count());
            self.extra_selections = extra;
            self.selection_revision += 1;
        }
        self.path = source.path.clone();
        self.saved_text = source.saved_text.clone();
        self.saved_line_ending = source.saved_line_ending;
        self.line_ending = source.line_ending;
        self.encoding = source.encoding;
        self.modified_at = source.modified_at;
        self.syntax = source.syntax.clone();
        self.read_only = source.read_only;
        self.scratch = source.scratch;
        self.scratch_name = source.scratch_name.clone();
    }

    /// Records that the current text is what is on disk at `path`.
    pub fn mark_saved(&mut self, path: &Path, modified_at: Option<SystemTime>) {
        self.path = Some(path.to_path_buf());
        self.scratch_name = None;
        self.saved_text = self.text.clone();
        self.saved_line_ending = self.line_ending;
        self.modified_at = modified_at;
    }

    /// Takes the contents of a freshly read copy of the same file, keeping the caret.
    pub fn reload_from(&mut self, fresh: Document) {
        let caret = self.selection.head.min(fresh.text.chars().count());
        self.text = fresh.text;
        self.note_text_changed();
        self.saved_text = fresh.saved_text;
        self.encoding = fresh.encoding;
        self.line_ending = fresh.line_ending;
        self.saved_line_ending = fresh.line_ending;
        self.modified_at = fresh.modified_at;
        self.set_selection(Selection::caret(caret));
    }

    /// Puts back unsaved text (hot exit); the document stays dirty.
    pub fn restore_unsaved_text(&mut self, text: String) {
        self.text = text;
        self.note_text_changed();
    }

    /// The text as it was last read from or written to disk.
    pub fn saved_text(&self) -> &str {
        &self.saved_text
    }

    pub fn is_dirty(&self) -> bool {
        !self.read_only
            && !self.scratch
            && (self.text != self.saved_text || (self.path.is_some() && self.line_ending != self.saved_line_ending))
    }

    /// Tab title. Untitled buffers are named after their first line, as in Sublime.
    pub fn name(&self) -> String {
        if let Some(name) = &self.scratch_name {
            return name.clone();
        }
        if let Some(name) = self.path.as_ref().and_then(|p| p.file_name()) {
            return name.to_string_lossy().into_owned();
        }
        let first_line = self.text.lines().next().unwrap_or("").trim();
        if first_line.is_empty() {
            UNTITLED.into()
        } else {
            first_line.chars().take(40).collect()
        }
    }

    pub fn first_line(&self) -> &str {
        self.text.lines().next().unwrap_or("")
    }

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn selection_revision(&self) -> u64 {
        self.selection_revision
    }

    pub fn change_count(&self) -> u64 {
        self.change_count
    }

    /// Records that the text changed outside this type's methods (the text widget edits it in place).
    pub fn note_text_changed(&mut self) {
        self.change_count += 1;
        // An edited preview stays.
        self.transient = false;
        let Some(before) = &self.tracked_text else {
            return;
        };
        let mut prefix = common_prefix(before, &self.text);
        let mut suffix = common_suffix(&before[prefix..], &self.text[prefix..]);
        // Typing "h" before "hello" reads the same as typing it after the first "h". The caret,
        // which has not moved yet, tells which: regions must not take in what was typed before them.
        let caret = char_to_byte(before, self.selection.start());
        if caret < prefix && before.is_char_boundary(caret) && self.text.is_char_boundary(caret) {
            let at_caret = common_suffix(&before[caret..], &self.text[caret..]);
            if caret + at_caret == prefix + suffix {
                (prefix, suffix) = (caret, at_caret);
            }
        }
        if prefix + suffix < before.len() || prefix + suffix < self.text.len() {
            let (end, inserted) = (before.len() - suffix, self.text[prefix..self.text.len() - suffix].to_string());
            self.record_change(prefix, end, &inserted);
        }
    }

    /// Moves the regions over a change of the bytes `start..end` of the tracked text (the text
    /// before the change), logs it, and brings the tracked text up to date.
    fn record_change(&mut self, start: usize, end: usize, inserted: &str) {
        let Some(before) = &mut self.tracked_text else {
            return;
        };
        let (start_char, end_char) = (before[..start].chars().count(), before[..end].chars().count());
        if let Some(log) = &mut self.change_log {
            log.push(TextChange {
                start: start_char,
                end: end_char,
                start_position: position_of(before, start),
                end_position: position_of(before, end),
                text: inserted.to_string(),
            });
        }
        before.replace_range(start..end, inserted);
        let inserted = inserted.chars().count();
        let regions = self.region_sets.iter_mut().flat_map(|s| s.regions.iter_mut());
        for region in regions.chain(self.phantoms.iter_mut().map(|p| &mut p.region)) {
            *region = (
                moved_position(region.0, start_char, end_char, inserted),
                moved_position(region.1, start_char, end_char, inserted),
            );
        }
        // The field being visited takes in what is typed at its start or end (typing over its
        // selected default); the others move like regions.
        if let Some(snippet) = &mut self.snippet {
            let current = snippet.current;
            for (index, field) in snippet.fields.iter_mut().enumerate() {
                for (a, b) in field.iter_mut() {
                    let (low, high) = ((*a).min(*b), (*a).max(*b));
                    let (low, high) = if index == current && low <= end_char && high >= start_char {
                        let low = if low <= start_char { low } else { start_char + inserted };
                        let high = if high <= end_char { start_char + inserted } else { moved_position(high, start_char, end_char, inserted) };
                        (low, high)
                    } else {
                        (moved_position(low, start_char, end_char, inserted), moved_position(high, start_char, end_char, inserted))
                    };
                    (*a, *b) = (low, high.max(low));
                }
            }
        }
    }

    /// Starts visiting a snippet's fields: the first one is selected (with its mirrors, as more
    /// selections). With one field (just where the caret ends), there is nothing to visit.
    pub fn start_snippet(&mut self, fields: Vec<Vec<(usize, usize)>>) {
        let Some(first) = fields.first().cloned() else { return };
        self.select_field(&first);
        if fields.len() > 1 {
            self.tracked_text.get_or_insert_with(|| self.text.clone());
            self.snippet = Some(SnippetFields { fields, current: 0 });
        }
    }

    fn select_field(&mut self, field: &[(usize, usize)]) {
        let mut regions = field.iter().map(|&(a, b)| Selection::new(a, b));
        if let Some(primary) = regions.next() {
            self.set_selections(primary, regions.collect::<Vec<_>>());
        }
    }

    /// The snippet's fields while the selection is still in the field being visited.
    pub fn snippet(&self) -> Option<&SnippetFields> {
        let snippet = self.snippet.as_ref()?;
        let (start, end) = (self.selection.start(), self.selection.end());
        let inside = snippet.fields.get(snippet.current)?.iter().any(|&(a, b)| a.min(b) <= start && end <= a.max(b));
        inside.then_some(snippet)
    }

    /// Whether Tab (`forward`) or Shift+Tab has a field to go to.
    pub fn has_snippet_field(&self, forward: bool) -> bool {
        self.snippet().is_some_and(|s| if forward { s.current + 1 < s.fields.len() } else { s.current > 0 })
    }

    /// `next_field` / `prev_field`: selects the next (or previous) field. Reaching the last one,
    /// where the caret ends, finishes the snippet. False when there is no such field.
    pub fn move_to_snippet_field(&mut self, forward: bool) -> bool {
        if !self.has_snippet_field(forward) {
            self.clear_snippet();
            return false;
        }
        let Some(snippet) = &mut self.snippet else { return false };
        snippet.current = if forward { snippet.current + 1 } else { snippet.current - 1 };
        let (field, last) = (snippet.fields[snippet.current].clone(), snippet.current + 1 == snippet.fields.len());
        self.select_field(&field);
        if last {
            self.clear_snippet();
        }
        true
    }

    /// `clear_fields`: stops visiting the snippet's fields.
    pub fn clear_snippet(&mut self) {
        self.snippet = None;
        self.stop_tracking_if_unused();
    }

    /// From now on [`take_text_changes`](Self::take_text_changes) has every edit of the text.
    pub fn log_text_changes(&mut self) {
        if self.change_log.is_none() {
            self.change_log = Some(Vec::new());
            self.tracked_text.get_or_insert_with(|| self.text.clone());
        }
    }

    /// The edits since the last call.
    pub fn take_text_changes(&mut self) -> Vec<TextChange> {
        self.change_log.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The selections besides the primary one, in the order they were added.
    pub fn extra_selections(&self) -> &[Selection] {
        &self.extra_selections
    }

    /// Every selection (primary and extra), sorted by position.
    pub fn selections(&self) -> Vec<Selection> {
        let mut all = self.extra_selections.clone();
        all.push(self.selection);
        all.sort_by_key(|s| (s.start(), s.end()));
        all
    }

    /// Moves the selection on behalf of a command; the view will follow it.
    /// Leaves a single selection.
    pub fn set_selection(&mut self, selection: Selection) {
        self.extra_selections.clear();
        self.selection = selection.clamped(self.char_count());
        self.selection_revision += 1;
    }

    /// Keeps the current selections and makes `selection` the new primary one (Ctrl+D).
    pub fn add_selection(&mut self, selection: Selection) {
        let mut extra = std::mem::take(&mut self.extra_selections);
        extra.push(self.selection);
        self.set_selection(selection);
        self.extra_selections = extra;
    }

    /// Replaces every selection on behalf of a command; `primary` is the one the view follows.
    pub fn set_selections(&mut self, primary: Selection, others: impl IntoIterator<Item = Selection>) {
        self.set_selection(primary);
        let len = self.char_count();
        let mut extra: Vec<Selection> = others
            .into_iter()
            .map(|s| s.clamped(len))
            .filter(|s| *s != self.selection)
            .collect();
        extra.sort_by_key(|s| (s.start(), s.end()));
        extra.dedup();
        self.extra_selections = extra;
    }

    /// Drops the extra selections, keeping the primary one (Sublime's `single_selection`).
    pub fn single_selection(&mut self) {
        self.extra_selections.clear();
    }

    /// Records where the user put the caret in the view (the view already shows it).
    /// Moving the caret by hand leaves a single selection.
    pub fn sync_selection_from_view(&mut self, selection: Selection) {
        let selection = selection.clamped(self.char_count());
        if selection != self.selection {
            self.extra_selections.clear();
        }
        self.selection = selection;
    }

    /// Runs a text command on this document. The command returns the new selection, or `None`
    /// when it did nothing.
    pub fn edit(&mut self, command: impl FnOnce(&mut String, Selection) -> Option<Selection>) -> bool {
        if self.read_only {
            return false;
        }
        let selection = self.selection.clamped(self.char_count());
        match command(&mut self.text, selection) {
            Some(new_selection) => {
                self.note_text_changed();
                self.set_selection(new_selection);
                true
            }
            None => false,
        }
    }

    /// Replaces the chars `start..end` with `insert` (a plugin's `view.insert/erase/replace`) and
    /// moves every selection along, as Sublime does: positions after the change shift, positions
    /// inside it go to its end, and a caret where text is inserted ends up after the new text.
    /// Returns false for read-only documents.
    pub fn replace_range(&mut self, start: usize, end: usize, insert: &str) -> bool {
        if self.read_only {
            return false;
        }
        let len = self.char_count();
        let start = start.min(len);
        let end = end.clamp(start, len);
        let (start_byte, end_byte) = (char_to_byte(&self.text, start), char_to_byte(&self.text, end));
        self.text.replace_range(start_byte..end_byte, insert);
        self.change_count += 1;
        self.transient = false;
        // The exact change, not a comparison, which cannot tell which of two equal chars went.
        self.record_change(start_byte, end_byte, insert);
        let inserted = insert.chars().count();
        let map = |p: usize| moved_position(p, start, end, inserted);
        let moved = |s: &Selection| Selection::new(map(s.anchor), map(s.head));
        let primary = moved(&self.selection);
        let extra: Vec<Selection> = self.extra_selections.iter().map(moved).collect();
        self.set_selections(primary, extra);
        true
    }

    /// Adds a region set, replacing the one with the same key.
    pub fn add_regions(&mut self, set: RegionSet) {
        self.tracked_text.get_or_insert_with(|| self.text.clone());
        match self.region_sets.iter_mut().find(|s| s.key == set.key) {
            Some(existing) => *existing = set,
            None => self.region_sets.push(set),
        }
    }

    /// The regions of a set, kept within the text.
    pub fn regions(&self, key: &str) -> Vec<(usize, usize)> {
        let len = self.char_count();
        let set = self.region_sets.iter().find(|s| s.key == key);
        set.map(|s| s.regions.iter().map(|&(a, b)| (a.min(len), b.min(len))).collect()).unwrap_or_default()
    }

    pub fn erase_regions(&mut self, key: &str) {
        self.region_sets.retain(|s| s.key != key);
        self.stop_tracking_if_unused();
    }

    pub fn region_sets(&self) -> &[RegionSet] {
        &self.region_sets
    }

    /// The region sets a plugin asked to keep across runs (`sublime.PERSISTENT`).
    pub fn persistent_regions(&self) -> Vec<RegionSet> {
        self.region_sets.iter().filter(|set| set.flags & PERSISTENT != 0).cloned().collect()
    }

    /// Puts back the region sets of the session.
    pub fn restore_regions(&mut self, sets: Vec<RegionSet>) {
        for set in sets {
            self.add_regions(set);
        }
    }

    /// Nothing needs the copy of the text any more.
    fn stop_tracking_if_unused(&mut self) {
        if self.region_sets.is_empty() && self.phantoms.is_empty() && self.change_log.is_none() && self.snippet.is_none() {
            self.tracked_text = None;
        }
    }

    /// Adds a phantom (its `id` given by the caller).
    pub fn add_phantom(&mut self, phantom: Phantom) {
        self.tracked_text.get_or_insert_with(|| self.text.clone());
        self.phantoms.push(phantom);
    }

    /// Erases the phantoms with that key.
    pub fn erase_phantoms(&mut self, key: &str) {
        self.phantoms.retain(|p| p.key != key);
        self.stop_tracking_if_unused();
    }

    pub fn erase_phantom(&mut self, id: u64) {
        self.phantoms.retain(|p| p.id != id);
        self.stop_tracking_if_unused();
    }

    /// The phantoms, their regions kept within the text.
    pub fn phantoms(&self) -> Vec<Phantom> {
        let len = self.char_count();
        self.phantoms
            .iter()
            .map(|p| Phantom { region: (p.region.0.min(len), p.region.1.min(len)), ..p.clone() })
            .collect()
    }

    /// The view reports which chars are on screen.
    pub fn set_visible_range(&mut self, start: usize, end: usize) {
        self.visible_range = Some((start, end));
    }

    /// The chars on screen; the whole text until the view has drawn the document.
    pub fn visible_range(&self) -> (usize, usize) {
        let len = self.char_count();
        self.visible_range.map_or((0, len), |(start, end)| (start.min(len), end.min(len)))
    }

    /// Asks the view to scroll so that `point` is visible (centered when `center`).
    pub fn request_show(&mut self, point: usize, center: bool) {
        self.show_request = Some(ShowRequest { point: point.min(self.char_count()), center });
        self.show_revision += 1;
    }

    pub fn show_request(&self) -> Option<ShowRequest> {
        self.show_request
    }

    /// Increases with every `request_show`, so the view knows when to scroll.
    pub fn show_revision(&self) -> u64 {
        self.show_revision
    }

    /// Runs a text command on every selection at once. The command gets the selections sorted
    /// by position and returns the new ones in the same order.
    pub fn edit_all(&mut self, command: impl FnOnce(&mut String, &[Selection]) -> Vec<Selection>) -> bool {
        if self.read_only {
            return false;
        }
        let len = self.char_count();
        let selections: Vec<Selection> = self.selections().iter().map(|s| s.clamped(len)).collect();
        let primary = self.selection.clamped(len);
        let primary_index = selections.iter().position(|s| *s == primary).unwrap_or(0);
        let mut new = command(&mut self.text, &selections);
        if new.len() != selections.len() {
            return false;
        }
        self.note_text_changed();
        let new_primary = new.remove(primary_index);
        self.set_selection(new_primary);
        // Carets that met (e.g. deleting the text between them) become one.
        new.dedup();
        new.retain(|s| *s != self.selection);
        self.extra_selections = new;
        true
    }
}

/// Where a position goes when the chars `start..end` become `inserted` chars: positions after the
/// change shift, positions inside it go to its end, and a caret where text is inserted ends up
/// after the new text.
fn moved_position(p: usize, start: usize, end: usize, inserted: usize) -> usize {
    if p < start || (p == start && start < end) {
        p
    } else if p >= end {
        p - (end - start) + inserted
    } else {
        start + inserted
    }
}

/// How many bytes two texts share at the start, on a char boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    let mut same = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    while !a.is_char_boundary(same) || !b.is_char_boundary(same) {
        same -= 1;
    }
    same
}

/// How many bytes two texts share at the end, on a char boundary.
fn common_suffix(a: &str, b: &str) -> usize {
    let mut same = a.bytes().rev().zip(b.bytes().rev()).take_while(|(x, y)| x == y).count();
    while !a.is_char_boundary(a.len() - same) || !b.is_char_boundary(b.len() - same) {
        same -= 1;
    }
    same
}

/// Zero-based line and column (in chars) of a byte offset.
fn position_of(text: &str, byte: usize) -> (usize, usize) {
    let before = &text[..byte];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (before.matches('\n').count(), before[line_start..].chars().count())
}

fn decode(bytes: Vec<u8>) -> (String, Encoding) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8WithBom);
    }
    match String::from_utf8(bytes) {
        Ok(text) => (text, Encoding::Utf8),
        Err(e) => (e.into_bytes().into_iter().map(char::from).collect(), Encoding::Latin1),
    }
}
