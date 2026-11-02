//! Presentation-only state: what is open on screen. It is not part of the application state
//! (the use cases neither know nor care whether the side bar is visible).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use entities::command::Command;
use use_cases::plugin_hooks::CommandTarget;
use use_cases::search::{Candidate, SearchKind};

/// The quick panel at the top of the window (Command Palette, Goto Anything, pickers).
pub struct QuickPanel {
    pub kind: SearchKind,
    pub query: String,
    pub selected: usize,
    /// Results for `results_query`; recomputed when the query changes.
    pub results: Vec<Candidate>,
    pub results_query: Option<String>,
    /// Color scheme to restore if the color scheme picker is cancelled.
    pub color_scheme_before: Option<String>,
    /// The query field should take keyboard focus (set when the panel opens).
    pub focus_requested: bool,
}

impl QuickPanel {
    pub fn new(kind: SearchKind, query: String) -> Self {
        Self {
            kind,
            query,
            selected: 0,
            results: Vec::new(),
            results_query: None,
            color_scheme_before: None,
            focus_requested: true,
        }
    }
}

/// The one-line panel at the bottom that a plugin opened (`window.show_input_panel`).
pub struct InputPanelState {
    pub focus_requested: bool,
}

#[derive(Default)]
pub struct FindPanelState {
    pub visible: bool,
    pub replace_mode: bool,
    /// The query field should take keyboard focus on the next frame.
    pub focus_requested: bool,
}

/// Edits the text widget performs itself (they must join its undo history).
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetEdit {
    Undo,
    Redo,
    SelectAll,
    Copy,
    Cut,
    Paste(String),
}

/// Requests from the controllers to the window / widgets, carried out by the framework layer.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewRequest {
    WidgetEdit(WidgetEdit),
    SetFullscreen(bool),
    CloseWindow,
}

pub struct ViewState {
    pub show_side_bar: bool,
    pub show_status_bar: bool,
    pub fullscreen: bool,
    pub quick_panel: Option<QuickPanel>,
    pub find_panel: FindPanelState,
    pub input_panel: Option<InputPanelState>,
    pub show_about: bool,
    pub show_load_errors: bool,
    pub expanded_folders: HashSet<PathBuf>,
    /// The text area should take keyboard focus on the next frame.
    pub focus_editor: bool,
    /// The text area had the keyboard focus last frame (set by the framework before keys are handled).
    pub text_has_focus: bool,
    /// The user already agreed to quit with unsaved changes.
    pub exit_confirmed: bool,
    /// A menu of the main menu is open (plugin commands are asked about when it opens).
    pub(crate) menu_open: bool,
    /// Side bar folder contents, read once per folder (cleared by "Refresh Folders").
    pub(crate) folder_listings: HashMap<PathBuf, Vec<(PathBuf, bool)>>,
    pub(crate) commands: VecDeque<Command>,
    pub(crate) requests: Vec<ViewRequest>,
    /// Commands done by the text widget whose `on_post_*_command` waits for the widget.
    pub(crate) widget_edits_in_progress: Vec<WidgetEditInProgress>,
}

/// A widget edit reaches the widget in the next frame's input, after the frame that asked for it.
pub(crate) struct WidgetEditInProgress {
    pub command: Command,
    pub target: CommandTarget,
    pub applied_next_frame: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            show_side_bar: true,
            show_status_bar: true,
            fullscreen: false,
            quick_panel: None,
            find_panel: FindPanelState::default(),
            input_panel: None,
            show_about: false,
            show_load_errors: false,
            expanded_folders: HashSet::new(),
            focus_editor: true,
            text_has_focus: false,
            exit_confirmed: false,
            menu_open: false,
            folder_listings: HashMap::new(),
            commands: VecDeque::new(),
            requests: Vec::new(),
            widget_edits_in_progress: Vec::new(),
        }
    }
}

impl ViewState {
    /// Queues a command to run before the next frame is drawn.
    pub fn queue_command(&mut self, command: Command) {
        self.commands.push_back(command);
    }

    pub(crate) fn widget_edits_requested(&self) -> usize {
        self.requests.iter().filter(|r| matches!(r, ViewRequest::WidgetEdit(_))).count()
    }

    /// Every frame, after the widgets: the edits the widget has done by now.
    pub(crate) fn take_finished_widget_edits(&mut self) -> Vec<(Command, CommandTarget)> {
        let (finished, waiting) = std::mem::take(&mut self.widget_edits_in_progress).into_iter().partition(|e| !e.applied_next_frame);
        self.widget_edits_in_progress = waiting;
        self.widget_edits_in_progress.iter_mut().for_each(|e| e.applied_next_frame = false);
        let finished: Vec<WidgetEditInProgress> = finished;
        finished.into_iter().map(|e| (e.command, e.target)).collect()
    }

    pub fn take_commands(&mut self) -> VecDeque<Command> {
        std::mem::take(&mut self.commands)
    }

    pub fn request(&mut self, request: ViewRequest) {
        self.requests.push(request);
    }

    pub fn take_requests(&mut self) -> Vec<ViewRequest> {
        std::mem::take(&mut self.requests)
    }

    pub fn forget_folder_listings(&mut self) {
        self.folder_listings.clear();
    }

    /// Flags saved with the session.
    pub fn layout(&self) -> Vec<(String, bool)> {
        vec![
            ("side_bar".into(), self.show_side_bar),
            ("status_bar".into(), self.show_status_bar),
        ]
    }

    pub fn apply_layout(&mut self, layout: &[(String, bool)]) {
        for (key, value) in layout {
            match key.as_str() {
                "side_bar" => self.show_side_bar = *value,
                "status_bar" => self.show_status_bar = *value,
                _ => {}
            }
        }
    }
}
