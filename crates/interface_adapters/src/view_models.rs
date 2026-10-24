//! View models: ready-to-draw data built by the presenters. Views only read these.

use std::ops::Range;
use std::path::PathBuf;

use entities::command::Command;
use entities::document::DocumentId;
use entities::selection::Selection;

pub struct TabViewModel {
    pub title: String,
    pub tooltip: String,
    pub dirty: bool,
    pub active: bool,
}

pub struct OpenFileViewModel {
    pub name: String,
    pub dirty: bool,
    pub active: bool,
}

pub enum TreeRowKind {
    Folder { expanded: bool, is_root: bool },
    File,
}

/// One visible row of the folder tree (already flattened and indented).
pub struct TreeRowViewModel {
    pub depth: usize,
    pub name: String,
    pub path: PathBuf,
    pub kind: TreeRowKind,
    pub active: bool,
}

pub struct SideBarViewModel {
    pub open_files: Vec<OpenFileViewModel>,
    pub folders: Vec<TreeRowViewModel>,
}

pub struct StatusBarViewModel {
    /// e.g. `Line 3, Column 7; 5 characters selected`.
    pub position: String,
    pub message: Option<String>,
    pub indentation: String,
    pub line_ending: String,
    pub encoding: String,
    pub syntax: String,
}

pub struct MenuEntryViewModel {
    pub caption: String,
    pub separator: bool,
    pub shortcut: Option<String>,
    /// `Some(checked)` for checkbox items.
    pub checked: Option<bool>,
    pub command: Option<Command>,
    pub children: Vec<MenuEntryViewModel>,
}

pub struct QuickPanelItemViewModel {
    pub caption: String,
    pub detail: String,
    pub hint: String,
}

pub struct QuickPanelViewModel {
    pub placeholder: &'static str,
    pub query: String,
    pub selected: usize,
    pub items: Vec<QuickPanelItemViewModel>,
    /// Focus the query field (with the caret after any preset text, e.g. `:`).
    pub take_focus: bool,
}

pub struct FindPanelViewModel {
    pub query: String,
    pub replacement: String,
    pub replace_mode: bool,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub wrap: bool,
    /// e.g. `4 matches`, or the regex error.
    pub status: String,
    pub has_error: bool,
    pub focus_requested: bool,
}

pub struct EditorViewModel {
    pub document: DocumentId,
    pub read_only: bool,
    pub syntax: String,
    pub color_scheme: String,
    pub selection: Selection,
    pub selection_revision: u64,
    /// Font size in points (Sublime's unit).
    pub font_size: f32,
    pub tab_size: usize,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub highlight_line: bool,
    pub scroll_past_end: bool,
    pub rulers: Vec<usize>,
    /// Enter is handled by the `auto_indent` text command instead of the widget.
    pub auto_indent: bool,
    /// Byte ranges to highlight (find panel matches).
    pub find_matches: Vec<Range<usize>>,
    pub take_focus: bool,
}

pub enum DialogViewModel {
    SaveChanges { document_name: String },
    ExitWithUnsavedChanges,
}

pub struct AboutViewModel {
    pub version: &'static str,
    pub packages: String,
    pub sublime_packages: Option<String>,
}
