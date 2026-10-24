//! Handles what the user does in the views: clicks, typing in panels, dialog answers.
//! Views report these as [`UiEvent`]s instead of changing state themselves.

use std::path::PathBuf;

use entities::command::Command;
use entities::document::{DocumentId, LineEnding};
use entities::find::FindOptions;
use entities::selection::Selection;
use use_cases::editing::TextCommand;
use use_cases::editor::CloseChoice;
use use_cases::search::SearchKind;
use use_cases::Editor;

use super::command_controller;
use crate::view_state::{ViewRequest, ViewState};

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// A menu item or anything else that maps to a Sublime command.
    RunCommand(Command),

    ActivateTab(usize),
    CloseTab(usize),
    NewTab,

    ToggleFolder(PathBuf),
    OpenFile(PathBuf),
    RemoveFolder(PathBuf),

    QuickPanelQueryChanged(String),
    QuickPanelHighlighted(usize),
    QuickPanelAccepted(usize),
    QuickPanelDismissed,

    FindOptionsChanged(FindOptions),
    ReplacementChanged(String),
    FindNext,
    FindPrevious,
    ReplaceNext,
    ReplaceAll,
    CloseFindPanel,

    SyntaxClicked,
    IndentationClicked,
    LineEndingClicked,

    /// The user moved the caret or selection in the text widget.
    SelectionChanged(DocumentId, Selection),
    /// Keys the editor handles with Sublime's indentation rules instead of the widget's.
    TabPressed,
    ShiftTabPressed,
    EnterPressed,

    CloseDialogAnswered(CloseChoice),
    AboutClosed,
    LoadErrorsClosed,
}

pub fn handle(editor: &mut Editor, view: &mut ViewState, event: UiEvent) {
    match event {
        UiEvent::RunCommand(command) => view.queue_command(command),

        UiEvent::ActivateTab(index) => {
            editor.activate(index);
            view.focus_editor = true;
        }
        UiEvent::CloseTab(index) => {
            if let Some(id) = editor.state().documents.get(index).map(|d| d.id) {
                editor.close_documents(vec![id]);
            }
        }
        UiEvent::NewTab => editor.new_file(),

        UiEvent::ToggleFolder(folder) => {
            if !view.expanded_folders.remove(&folder) {
                view.expanded_folders.insert(folder);
            }
        }
        UiEvent::OpenFile(path) => {
            editor.open_file(&path);
            view.focus_editor = true;
        }
        UiEvent::RemoveFolder(folder) => editor.remove_folder(&folder),

        UiEvent::QuickPanelQueryChanged(query) => {
            if let Some(panel) = &mut view.quick_panel {
                panel.query = query;
                panel.selected = 0;
                // Goto Line moves the caret while typing, like Sublime.
                if panel.kind == SearchKind::Goto && panel.query.starts_with(':') {
                    if let Some(first) = editor.search(SearchKind::Goto, &panel.query).first() {
                        editor.preview_candidate(&first.target);
                    }
                }
            }
        }
        UiEvent::QuickPanelHighlighted(index) => {
            if let Some(panel) = &mut view.quick_panel {
                panel.selected = index;
                if let Some(candidate) = panel.results.get(index) {
                    if panel.kind == SearchKind::ColorSchemes {
                        editor.preview_candidate(&candidate.target);
                    }
                }
            }
        }
        UiEvent::QuickPanelAccepted(index) => {
            if let Some(panel) = view.quick_panel.take() {
                if let Some(candidate) = panel.results.into_iter().nth(index) {
                    if let Some(command) = editor.accept_candidate(candidate.target) {
                        view.queue_command(command);
                    }
                }
            }
            view.focus_editor = true;
        }
        UiEvent::QuickPanelDismissed => command_controller::close_quick_panel(editor, view),

        UiEvent::FindOptionsChanged(options) => editor.set_find_options(options),
        UiEvent::ReplacementChanged(replacement) => editor.set_replacement(replacement),
        UiEvent::FindNext => view.queue_command(Command::new("find_next")),
        UiEvent::FindPrevious => view.queue_command(Command::new("find_prev")),
        UiEvent::ReplaceNext => view.queue_command(Command::new("replace_next")),
        UiEvent::ReplaceAll => view.queue_command(Command::new("replace_all")),
        UiEvent::CloseFindPanel => view.queue_command(Command::new("hide_panel")),

        UiEvent::SyntaxClicked => view.queue_command(Command::with_args(
            "show_overlay",
            serde_json::json!({ "overlay": "command_palette", "text": "Set Syntax: " }),
        )),
        UiEvent::IndentationClicked => view.queue_command(Command::with_args(
            "toggle_setting",
            serde_json::json!({ "setting": "translate_tabs_to_spaces" }),
        )),
        UiEvent::LineEndingClicked => {
            let next = match editor.active_document().map(|d| d.line_ending) {
                Some(LineEnding::Windows) => "unix",
                _ => "windows",
            };
            view.queue_command(Command::with_args("set_line_ending", serde_json::json!({ "type": next })));
        }

        UiEvent::SelectionChanged(document, selection) => editor.sync_selection_from_view(document, selection),
        UiEvent::TabPressed => editor.run_text_command(TextCommand::InsertTab),
        UiEvent::ShiftTabPressed => editor.run_text_command(TextCommand::Unindent),
        UiEvent::EnterPressed => editor.run_text_command(TextCommand::NewlineAutoIndent),

        UiEvent::CloseDialogAnswered(choice) => {
            if editor.resolve_pending_close(choice) {
                view.exit_confirmed = true;
                view.request(ViewRequest::CloseWindow);
            }
        }
        UiEvent::AboutClosed => view.show_about = false,
        UiEvent::LoadErrorsClosed => view.show_load_errors = false,
    }
}
