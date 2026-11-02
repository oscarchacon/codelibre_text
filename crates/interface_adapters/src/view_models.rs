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
    /// What plugins show for the active view (`view.set_status`), in key order.
    pub plugin_statuses: Vec<String>,
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
    pub enabled: bool,
    pub command: Option<Command>,
    pub children: Vec<MenuEntryViewModel>,
}

pub struct QuickPanelItemViewModel {
    pub caption: String,
    pub detail: String,
    pub hint: String,
    /// A letter shown in a badge before the caption; empty for none.
    pub kind: String,
}

pub struct QuickPanelViewModel {
    pub placeholder: String,
    /// A click outside does not close the panel.
    pub keep_open: bool,
    /// Shown instead of the list when nothing matches.
    pub no_matches: String,
    pub query: String,
    pub selected: usize,
    pub items: Vec<QuickPanelItemViewModel>,
    /// Focus the query field (with the caret after any preset text, e.g. `:`).
    pub take_focus: bool,
    /// Items in the monospace font (a plugin's `sublime.MONOSPACE_FONT`).
    pub monospace: bool,
}

pub struct InputPanelViewModel {
    /// The panel's view, so the caret of a closed panel is not taken for this one's.
    pub id: u64,
    pub caption: String,
    pub text: String,
    /// In chars. The view moves its caret here when `selection_revision` changes.
    pub selection: (usize, usize),
    pub selection_revision: u64,
    pub take_focus: bool,
}

/// The output panel shown at the bottom (a build's or a plugin's output).
pub struct OutputPanelViewModel {
    pub name: String,
    /// The end of the panel's text (at most [`presenters::output_panel::MAX_LINES`] lines).
    ///
    /// [`presenters::output_panel::MAX_LINES`]: crate::presenters::output_panel::MAX_LINES
    pub text: String,
    pub font_size: f32,
    /// The line of the panel's text that `text` starts with.
    pub first_line: usize,
    /// The line of `text` of the result visited last.
    pub current_line: Option<usize>,
    /// Changes when a result is visited: the view scrolls to it.
    pub result_revision: u64,
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
    /// Selections besides the primary one (Ctrl+D); the view paints them itself.
    pub extra_selections: Vec<Selection>,
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
    /// Regions plugins marked (`view.add_regions`), except hidden ones.
    pub regions: Vec<RegionViewModel>,
    /// A plugin's `view.show`: the char to scroll to, whether centered, and a counter that
    /// changes with each request.
    pub show: Option<(usize, bool, u64)>,
    pub take_focus: bool,
}

/// How a plugin's region set is drawn (from Sublime's `DRAW_*` flags).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionViewModel {
    /// Char ranges, sorted by start.
    pub ranges: Vec<Range<usize>>,
    /// The color comes from this scope in the color scheme (the view resolves it).
    pub scope: String,
    /// Gutter icon on each region's first line: `dot`, `circle`, `bookmark`… (any other name is drawn as a dot).
    pub icon: String,
    pub fill: bool,
    pub outline: bool,
    pub underline: Option<Underline>,
    /// Empty regions are drawn (as a bar where the caret would be).
    pub draw_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    Solid,
    Stippled,
    Squiggly,
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
