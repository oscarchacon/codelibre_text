//! View models: ready-to-draw data built by the presenters. Views only read these.

use std::ops::Range;
use std::path::PathBuf;

use entities::command::Command;
use entities::document::DocumentId;
use entities::keymap::KeyCombo;
use entities::minihtml;
use entities::selection::Selection;

pub struct TabViewModel {
    /// The document's index in the window (for `UiEvent::ActivateTab` / `CloseTab`).
    pub index: usize,
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

#[derive(Clone, PartialEq)]
pub struct MenuEntryViewModel {
    pub caption: String,
    /// The `id` packages extend it by (`"preferences"`, `"help"`…).
    pub id: Option<String>,
    pub separator: bool,
    pub shortcut: Option<String>,
    /// The key of `shortcut`, unless it is a chord (native menus take one key).
    pub key: Option<KeyCombo>,
    /// `Some(checked)` for checkbox items.
    pub checked: Option<bool>,
    pub enabled: bool,
    pub command: Option<Command>,
    pub children: Vec<MenuEntryViewModel>,
}

/// The menu bar of macOS: the application menu first (About, Preferences, the system's Services /
/// Hide items, Quit), then the main menu without the entries moved there.
#[derive(Clone, PartialEq)]
pub struct MacMenuViewModel {
    pub app_name: String,
    pub about: Option<MenuEntryViewModel>,
    pub preferences: Option<MenuEntryViewModel>,
    pub services: String,
    pub hide: String,
    pub hide_others: String,
    pub show_all: String,
    pub quit: Option<MenuEntryViewModel>,
    pub menus: Vec<MenuEntryViewModel>,
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

/// One pane of the window's layout: its tabs and the document it shows.
pub struct GroupViewModel {
    pub group: usize,
    /// Where it is, as fractions of the editor area: `[left, top, right, bottom]`.
    pub rect: [f32; 4],
    /// It has the focus.
    pub active: bool,
    pub tabs: Vec<TabViewModel>,
    pub editor: Option<EditorViewModel>,
}

pub struct EditorViewModel {
    pub document: DocumentId,
    pub group: usize,
    /// The document of the focused group: it takes the keyboard focus. Clicking another group's
    /// text gives that group the focus.
    pub active: bool,
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
    /// A plugin's popup on this view.
    pub popup: Option<PopupViewModel>,
    /// Plugins' phantoms, by position.
    pub phantoms: Vec<PhantomViewModel>,
    /// Some plugin listens to `on_hover`: the view reports where the mouse rests.
    pub report_hover: bool,
    pub auto_complete: Option<AutoCompleteViewModel>,
}

/// The auto-complete list, under the typed word.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoCompleteViewModel {
    /// The char where the typed word starts: the list opens below it.
    pub location: usize,
    /// The first rows (a long list is cut: typing narrows it).
    pub items: Vec<CompletionViewModel>,
    pub selected: usize,
    /// The selected completion's `details` (minihtml), shown under the list.
    pub details: Vec<minihtml::Line>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionViewModel {
    pub trigger: String,
    /// Chars of `trigger` the typed text matched (drawn highlighted).
    pub matched: Vec<usize>,
    pub annotation: String,
    /// The badge's letter (empty: no badge) and the kind's name.
    pub kind_letter: String,
    pub kind_name: String,
    /// Sublime's `KindId`, which picks the badge's color.
    pub kind_id: u8,
}

/// A plugin's popup (`view.show_popup`), its minihtml read into lines.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupViewModel {
    /// The char it points at: it opens below it.
    pub location: usize,
    pub lines: Vec<minihtml::Line>,
    /// In pixels.
    pub max_width: f32,
    pub max_height: f32,
    /// It goes when the mouse moves away from it (`HIDE_ON_MOUSE_MOVE*`).
    pub hide_on_mouse_move_away: bool,
}

/// A plugin's phantom (`view.add_phantom`): drawn after the end of the line of `point`.
#[derive(Debug, Clone, PartialEq)]
pub struct PhantomViewModel {
    pub id: u64,
    pub point: usize,
    pub lines: Vec<minihtml::Line>,
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
