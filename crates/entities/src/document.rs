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

/// A named set of regions a plugin marks (a linter's errors, a search's matches…). The regions
/// move with the text.
#[derive(Debug, Clone, PartialEq)]
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
            selection: Selection::caret(0),
            extra_selections: Vec::new(),
            selection_revision: 1,
            change_count: 0,
            region_sets: Vec::new(),
            show_request: None,
            show_revision: 0,
            tracked_text: None,
            change_log: None,
            visible_range: None,
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
        for set in &mut self.region_sets {
            for region in &mut set.regions {
                *region = (
                    moved_position(region.0, start_char, end_char, inserted),
                    moved_position(region.1, start_char, end_char, inserted),
                );
            }
        }
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
        if self.region_sets.is_empty() && self.change_log.is_none() {
            self.tracked_text = None;
        }
    }

    pub fn region_sets(&self) -> &[RegionSet] {
        &self.region_sets
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
