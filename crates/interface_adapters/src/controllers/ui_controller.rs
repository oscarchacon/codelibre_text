//! Handles what the user does in the views: clicks, typing in panels, dialog answers.
//! Views report these as [`UiEvent`]s instead of changing state themselves.

use std::path::PathBuf;

use entities::command::Command;
use entities::document::{DocumentId, LineEnding};
use entities::find::FindOptions;
use entities::selection::Selection;
use entities::text_commands::CaretMotion;
use use_cases::editing::TextCommand;
use use_cases::editor::CloseChoice;
use use_cases::search::{CandidateTarget, SearchKind};
use use_cases::Editor;

use super::command_controller;
use crate::view_state::{ViewRequest, ViewState};

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// A menu item or anything else that maps to a Sublime command.
    RunCommand(Command),
    /// Every frame: whether a menu of the main menu is open.
    MenuOpen(bool),

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

    /// A plugin's input panel: the text changed, the caret moved, Enter was pressed.
    InputPanelChanged(String),
    InputPanelSelected { anchor: usize, head: usize },
    InputPanelDone,

    /// A line of the output panel was double-clicked (counting from the start of its text).
    OutputPanelLineOpened(usize),

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

    /// The text widget changed the document's text (typing, its own undo, paste…).
    TextEdited(DocumentId),
    /// The user moved the caret or selection in the text widget.
    SelectionChanged(DocumentId, Selection),
    /// Every frame: the chars on screen.
    VisibleRange(DocumentId, usize, usize),
    /// Keys the editor handles with Sublime's indentation rules instead of the widget's.
    TabPressed,
    ShiftTabPressed,
    EnterPressed,
    /// Keys the editor applies at every selection while there are several (Ctrl+D).
    TypedAtSelections(String),
    DeleteAtSelections { forward: bool },
    MoveCarets(CaretMotion),

    CloseDialogAnswered(CloseChoice),
    AboutClosed,
    LoadErrorsClosed,
}

pub fn handle(editor: &mut Editor, view: &mut ViewState, event: UiEvent) {
    match event {
        UiEvent::RunCommand(command) => view.queue_command(command),
        UiEvent::MenuOpen(open) => {
            if open && !view.menu_open {
                editor.refresh_plugin_command_states();
            }
            view.menu_open = open;
        }

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
            let mut plugin_item = None;
            if let Some(panel) = &mut view.quick_panel {
                panel.selected = index;
                match panel.results.get(index).map(|c| &c.target) {
                    Some(CandidateTarget::PluginItem(item)) => plugin_item = Some(*item),
                    Some(target) if panel.kind == SearchKind::ColorSchemes => editor.preview_candidate(target),
                    _ => {}
                }
            }
            if let Some(item) = plugin_item {
                command_controller::highlight_plugin_item(editor, view, item);
            }
        }
        UiEvent::QuickPanelAccepted(index) => {
            view.focus_editor = true;
            if let Some(panel) = view.quick_panel.take() {
                match panel.results.into_iter().nth(index).map(|c| c.target) {
                    Some(CandidateTarget::PluginItem(item)) => command_controller::answer_plugin_quick_panel(editor, view, Some(item)),
                    Some(target) => {
                        if let Some(command) = editor.accept_candidate(target) {
                            command_controller::run_from_palette(editor, view, command);
                        }
                    }
                    None if panel.kind == SearchKind::PluginItems => command_controller::answer_plugin_quick_panel(editor, view, None),
                    None => {}
                }
            }
        }
        UiEvent::QuickPanelDismissed => command_controller::close_quick_panel(editor, view),

        UiEvent::InputPanelChanged(text) => {
            if view.input_panel.is_some() {
                command_controller::change_plugin_input(editor, view, &text);
            }
        }
        UiEvent::InputPanelSelected { anchor, head } => editor.select_in_plugin_input_panel(Selection::new(anchor, head)),
        UiEvent::InputPanelDone => command_controller::finish_plugin_input(editor, view, true),

        UiEvent::OutputPanelLineOpened(line) => {
            if editor.open_result_at_line(line) {
                view.focus_editor = true;
            }
        }

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

        UiEvent::TextEdited(document) => editor.note_text_edited(document),
        UiEvent::SelectionChanged(document, selection) => editor.sync_selection_from_view(document, selection),
        UiEvent::VisibleRange(document, start, end) => editor.sync_visible_range(document, start, end),
        UiEvent::TabPressed => editor.run_text_command(TextCommand::InsertTab),
        UiEvent::ShiftTabPressed => editor.run_text_command(TextCommand::Unindent),
        UiEvent::EnterPressed => editor.run_text_command(TextCommand::NewlineAutoIndent),
        UiEvent::TypedAtSelections(text) => editor.insert_at_selections(&text),
        UiEvent::DeleteAtSelections { forward } => editor.delete_at_selections(forward),
        UiEvent::MoveCarets(motion) => editor.move_carets(motion),

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
