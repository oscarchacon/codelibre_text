//! An open buffer: its text, the file it belongs to, how that file is encoded, and the selection.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::selection::Selection;

pub type DocumentId = u64;

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
    selection: Selection,
    /// Increases whenever the selection is changed by a command (not by the user typing),
    /// so the view knows it has to move its caret.
    selection_revision: u64,
}

impl Document {
    pub fn new_untitled(id: DocumentId, line_ending: LineEnding) -> Self {
        Self {
            id,
            path: None,
            scratch_name: None,
            read_only: false,
            text: String::new(),
            saved_text: String::new(),
            line_ending,
            saved_line_ending: line_ending,
            encoding: Encoding::Utf8,
            syntax: "Plain Text".into(),
            modified_at: None,
            selection: Selection::caret(0),
            selection_revision: 1,
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
    }

    pub fn is_dirty(&self) -> bool {
        !self.read_only
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
            "untitled".into()
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

    /// Moves the selection on behalf of a command; the view will follow it.
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection.clamped(self.char_count());
        self.selection_revision += 1;
    }

    /// Records where the user put the caret in the view (the view already shows it).
    pub fn sync_selection_from_view(&mut self, selection: Selection) {
        self.selection = selection.clamped(self.char_count());
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
                self.set_selection(new_selection);
                true
            }
            None => false,
        }
    }
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
