//! Handles what the user does in the views: clicks, typing in panels, dialog answers.
//! Views report these as [`UiEvent`]s instead of changing state themselves.

use std::path::PathBuf;

use entities::command::Command;
use entities::document::{DocumentId, LineEnding};
use entities::find::FindOptions;
use entities::menu::ContextMenuTarget;
use entities::selection::Selection;
use entities::text_commands::CaretMotion;
use serde_json::Value;
use use_cases::editing::TextCommand;
use use_cases::editor::CloseChoice;
use use_cases::plugin_popups::HoverZone;
use use_cases::search::{CandidateTarget, SearchKind};
use use_cases::Editor;

use super::command_controller;
use crate::presenters;
use crate::view_state::{ViewRequest, ViewState, WidgetEdit};

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// A menu item or anything else that maps to a Sublime command.
    RunCommand(Command),
    /// Every frame: whether a menu of the main menu is open.
    MenuOpen(bool),
    /// Every frame: what the open context menu was opened on (a view pushes it while its menu is
    /// open; the window pushes `None` when no menu is).
    ContextMenu(Option<ContextMenuTarget>),

    ActivateTab(usize),
    CloseTab(usize),
    NewTab,
    /// A group of the layout was clicked (its text, its tab strip or its empty area).
    FocusGroup(usize),
    /// The line between two columns (`vertical`) or rows was dragged: boundary `index` is now at
    /// `fraction` of the window.
    LayoutBoundaryDragged {
        vertical: bool,
        index: usize,
        fraction: f64,
    },
    /// A tab (the document at `tab` in the window) was dropped on `group`, at `index` among its tabs.
    TabDropped {
        tab: usize,
        group: usize,
        index: usize,
    },

    /// A double click on a preview's tab: it stays open as a tab of its own.
    KeepTab(usize),

    ToggleFolder(PathBuf),
    /// The console's input field changed.
    ConsoleInputChanged(String),
    /// Enter in the console's field: its line runs as Python.
    ConsoleInputEntered,
    /// Up (`true`) or Down in the console's field: the previous or next line run.
    ConsoleHistory(bool),
    /// A row of the side bar's tree clicked: it alone is selected; with `toggle` (Ctrl, Cmd on
    /// macOS) it joins or leaves the selection; with `range` (Shift) the rows from the last
    /// one clicked to it are.
    SideBarSelect {
        path: PathBuf,
        toggle: bool,
        range: bool,
    },
    OpenFile(PathBuf),
    /// A file clicked once in the side bar: a preview (a double click opens it for good).
    PreviewFile(PathBuf),

    QuickPanelQueryChanged(String),
    QuickPanelHighlighted(usize),
    QuickPanelAccepted(usize),
    /// A plugin quick-panel item was accepted; the event carries the modifiers held at selection.
    PluginQuickPanelAccepted {
        index: usize,
        event: Value,
    },
    QuickPanelDismissed,
    /// A link in an item's details was clicked (its `href`): it opens in the browser.
    QuickPanelLinkClicked(String),

    /// A plugin's input panel: the text changed, the caret moved, Enter was pressed.
    InputPanelChanged(String),
    InputPanelSelected {
        anchor: usize,
        head: usize,
    },
    InputPanelDone,
    /// A plugin input panel was submitted with the event modifiers.
    PluginInputPanelDone(Value),
    /// Backspace in the empty field of a panel an input handler asks in (the quick or the input
    /// panel): back to the previous argument.
    PluginPanelBack,

    /// A line of the output panel was double-clicked (counting from the start of its text).
    OutputPanelLineOpened(usize),
    /// A click in the output panel gave its view focus.
    OutputPanelFocused(DocumentId),

    /// A link in a plugin's popup or phantom was clicked (its `href`).
    PopupLinkClicked(String),
    PhantomLinkClicked {
        phantom: u64,
        href: String,
    },
    /// A link in an annotation of the region set `key` was clicked.
    AnnotationLinkClicked {
        document: DocumentId,
        key: String,
        href: String,
    },
    /// Esc, or the mouse moved away from a popup that asked for it.
    PopupDismissed,
    /// An item of a plugin's popup menu was chosen (`None`: it was dismissed).
    PopupMenuChosen(Option<usize>),
    /// The mouse rests over a view (for `on_hover`).
    Hover {
        document: DocumentId,
        point: usize,
        zone: HoverZone,
    },

    FindOptionsChanged(FindOptions),
    ReplacementChanged(String),
    /// Find in Files' "Where" field changed.
    FindWhereChanged(String),
    /// Find in Files' "Use Buffer" toggle: its results in a tab.
    FindUseBufferChanged(bool),
    /// Find in Files' Find button (or Enter in its fields).
    FindAll,
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
    /// A click, or the start of a drag, in the text widget (before its `SelectionChanged`).
    TextClicked(DocumentId),
    /// A double click on the char at this point of a tab's text (a result opens, in a view
    /// with results).
    TextDoubleClicked(DocumentId, usize),
    /// The user moved the caret or selection in the text widget.
    SelectionChanged(DocumentId, Selection),
    /// The text widget typed these characters (after `TextEdited` and `SelectionChanged`): they
    /// may open the auto-complete list.
    TextTyped(DocumentId, String),
    /// A completion of the auto-complete list was clicked (its row).
    CompletionClicked(usize),
    /// Every frame: the chars on screen.
    VisibleRange(DocumentId, usize, usize),
    /// The "…" marker of the fold that starts at this char was clicked.
    Unfold(DocumentId, usize),
    /// A fold arrow in the gutter was clicked, on this line (0-based).
    ToggleFold(DocumentId, usize),
    /// The view scrolled where a plugin's `set_viewport_position` asked (before its geometry).
    ViewportScrolled(DocumentId),
    /// How the view lays its text out and where it is scrolled, every frame.
    ViewGeometry(DocumentId, entities::view_geometry::ViewGeometry),
    /// Keys the editor handles with Sublime's indentation rules instead of the widget's.
    TabPressed,
    ShiftTabPressed,
    EnterPressed,
    /// Keys the editor applies at every selection while there are several (Ctrl+D).
    TypedAtSelections(String),
    DeleteAtSelections {
        forward: bool,
    },
    MoveCarets(CaretMotion),

    CloseDialogAnswered(CloseChoice),
    AboutClosed,
    LoadErrorsClosed,
}

/// What a clipboard entry of a field's context menu does to the field (`paste` reads the
/// clipboard here); None for another command, which runs as usual.
fn field_edit(editor: &mut Editor, command: &Command) -> Option<WidgetEdit> {
    match command.name.as_str() {
        "cut" => Some(WidgetEdit::Cut),
        "copy" => Some(WidgetEdit::Copy),
        "paste" => editor.clipboard_text().map(WidgetEdit::Paste),
        "select_all" => Some(WidgetEdit::SelectAll),
        _ => None,
    }
}

pub fn handle(editor: &mut Editor, view: &mut ViewState, event: UiEvent) {
    match event {
        UiEvent::RunCommand(command) => match view.context_menu.clone() {
            // An entry of the context menu of an output panel runs on the panel's view.
            Some(ContextMenuTarget::Text { document, .. })
                if command_controller::run_in_panel_view(editor, view, document, &command) => {}
            // The clipboard entries of a field's menu are the field's own.
            Some(ContextMenuTarget::Widget { field }) => match field_edit(editor, &command) {
                Some(edit) => view.request(ViewRequest::FieldEdit { field, edit }),
                None => view.queue_command(command),
            },
            // An image's entries are about the picture clicked (in a group not focused too).
            Some(ContextMenuTarget::Image { document }) => {
                let index = editor
                    .state()
                    .documents
                    .iter()
                    .position(|d| d.id == document);
                if let Some(index) = index {
                    editor.activate(index);
                }
                view.queue_command(command);
            }
            _ => view.queue_command(command),
        },
        UiEvent::MenuOpen(open) => {
            if open && !view.menu_open {
                editor.refresh_plugin_command_states();
            }
            view.menu_open = open;
        }
        UiEvent::ContextMenu(target) => {
            if view.context_menu != target {
                if let Some(target) = &target {
                    editor.open_context_menu(target);
                }
                view.context_menu = target;
            }
        }

        UiEvent::ActivateTab(index) => {
            editor.blur_output_panel();
            view.focused_output_panel = None;
            view.output_panel_focus_requested = None;
            editor.activate(index);
            view.focus_editor = true;
        }
        UiEvent::CloseTab(index) => {
            if let Some(id) = editor.state().documents.get(index).map(|d| d.id) {
                editor.close_documents(vec![id]);
            }
        }
        UiEvent::NewTab => {
            editor.blur_output_panel();
            view.focused_output_panel = None;
            view.output_panel_focus_requested = None;
            editor.new_file();
        }
        UiEvent::FocusGroup(group) => {
            editor.blur_output_panel();
            view.focused_output_panel = None;
            view.output_panel_focus_requested = None;
            if group != editor.active_group() {
                editor.focus_group(group);
            }
            view.focus_editor = true;
        }
        UiEvent::LayoutBoundaryDragged {
            vertical,
            index,
            fraction,
        } => editor.move_layout_boundary(vertical, index, fraction),
        UiEvent::TabDropped { tab, group, index } => {
            editor.blur_output_panel();
            view.focused_output_panel = None;
            view.output_panel_focus_requested = None;
            if let Some(id) = editor.state().documents.get(tab).map(|d| d.id) {
                editor.drop_view_in_group(id, group, index);
                view.focus_editor = true;
            }
        }

        UiEvent::ToggleFolder(folder) => {
            if !view.expanded_folders.remove(&folder) {
                view.expanded_folders.insert(folder);
            }
        }
        UiEvent::ConsoleInputChanged(text) => {
            view.console_input = text;
            view.console_history_at = None;
        }
        UiEvent::ConsoleInputEntered => {
            let code = std::mem::take(&mut view.console_input);
            view.console_history_at = None;
            if !code.trim().is_empty() && view.console_history.last() != Some(&code) {
                view.console_history.push(code.clone());
            }
            editor.run_console_line(&code, &mut |editor: &mut Editor, command: &Command| {
                command_controller::run(editor, view, command)
            });
            view.console_focus_requested = true;
        }
        UiEvent::ConsoleHistory(back) => {
            let count = view.console_history.len();
            let at = match (view.console_history_at, back) {
                (None, true) => count.checked_sub(1),
                (None, false) => None,
                (Some(at), true) => Some(at.saturating_sub(1)),
                (Some(at), false) => (at + 1 < count).then_some(at + 1),
            };
            view.console_history_at = at;
            view.console_input = at
                .and_then(|at| view.console_history.get(at).cloned())
                .unwrap_or_default();
        }
        UiEvent::SideBarSelect {
            path,
            toggle,
            range,
        } => select_in_side_bar(editor, view, path, toggle, range),
        UiEvent::OpenFile(path) => {
            if editor.open_file(&path) {
                if let Some(id) = editor.active_document().map(|d| d.id) {
                    editor.keep_document(id);
                }
            }
            view.focus_editor = true;
        }
        UiEvent::PreviewFile(path) => {
            editor.preview_file(&path);
            view.focus_editor = true;
        }
        UiEvent::KeepTab(index) => {
            if let Some(id) = editor.state().documents.get(index).map(|d| d.id) {
                editor.keep_document(id);
                editor.activate(index);
            }
        }

        UiEvent::QuickPanelQueryChanged(query) => {
            let text_input = command_controller::palette_asks_text_input(editor, view);
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
            if text_input {
                if let Some(query) = view.quick_panel.as_ref().map(|panel| panel.query.clone()) {
                    command_controller::change_plugin_input(editor, view, &query);
                }
            }
        }
        UiEvent::QuickPanelHighlighted(index) => {
            let mut plugin_item = None;
            if let Some(panel) = &mut view.quick_panel {
                panel.selected = index;
                match panel.results.get(index).map(|c| &c.target) {
                    Some(CandidateTarget::PluginItem(item)) => plugin_item = Some(*item),
                    Some(target) if panel.kind == SearchKind::ColorSchemes => {
                        editor.preview_candidate(target)
                    }
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
                    Some(CandidateTarget::PluginItem(item)) => {
                        command_controller::answer_plugin_quick_panel(editor, view, Some(item))
                    }
                    Some(target) => {
                        if let Some(command) = editor.accept_candidate(target) {
                            command_controller::run_from_palette(editor, view, command);
                        }
                    }
                    None if panel.kind == SearchKind::PluginItems => {
                        command_controller::answer_plugin_quick_panel(editor, view, None)
                    }
                    None => {}
                }
            }
        }
        UiEvent::PluginQuickPanelAccepted { index, event } => {
            view.focus_editor = true;
            if let Some(panel) = view.quick_panel.take() {
                match panel.results.into_iter().nth(index).map(|c| c.target) {
                    Some(CandidateTarget::PluginItem(item)) => {
                        command_controller::answer_plugin_quick_panel_with_event(
                            editor,
                            view,
                            Some(item),
                            event,
                        )
                    }
                    None if panel.kind == SearchKind::PluginItems => {
                        command_controller::answer_plugin_quick_panel_with_event(
                            editor, view, None, event,
                        )
                    }
                    _ => {}
                }
            }
        }
        UiEvent::QuickPanelDismissed => command_controller::close_quick_panel(editor, view),
        UiEvent::QuickPanelLinkClicked(href) => editor.open_url(&href),

        UiEvent::InputPanelChanged(text) => {
            if view.input_panel.is_some() {
                command_controller::change_plugin_input(editor, view, &text);
            }
        }
        UiEvent::InputPanelSelected { anchor, head } => {
            editor.select_in_plugin_input_panel(Selection::new(anchor, head))
        }
        UiEvent::InputPanelDone => command_controller::finish_plugin_input(editor, view, true),
        UiEvent::PluginInputPanelDone(event) => {
            command_controller::finish_plugin_input_with_event(editor, view, true, event)
        }
        UiEvent::PluginPanelBack => command_controller::go_back_in_plugin_panel(editor, view),

        UiEvent::OutputPanelLineOpened(line) => {
            if editor.open_result_at_line(line) {
                editor.blur_output_panel();
                view.focused_output_panel = None;
                view.output_panel_focus_requested = None;
                view.focus_editor = true;
            }
        }
        UiEvent::OutputPanelFocused(document) => {
            if editor.focus_output_panel(document) {
                view.focused_output_panel = Some(document);
                view.output_panel_focus_requested = Some(document);
            }
            view.focus_editor = false;
        }

        UiEvent::PopupLinkClicked(href) => {
            command_controller::navigate_plugin_popup(editor, view, &href)
        }
        UiEvent::PhantomLinkClicked { phantom, href } => {
            command_controller::navigate_plugin_phantom(editor, view, phantom, &href)
        }
        UiEvent::AnnotationLinkClicked {
            document,
            key,
            href,
        } => command_controller::navigate_plugin_annotation(editor, view, document, &key, &href),
        UiEvent::PopupDismissed => editor.hide_plugin_popup(),
        UiEvent::PopupMenuChosen(index) => {
            command_controller::choose_plugin_popup_menu(editor, view, index)
        }
        UiEvent::Hover {
            document,
            point,
            zone,
        } => editor.plugin_hover(document, point, zone),

        UiEvent::FindOptionsChanged(options) => editor.set_find_options(options),
        UiEvent::ReplacementChanged(replacement) => editor.set_replacement(replacement),
        UiEvent::FindWhereChanged(text) => editor.set_find_where(text),
        UiEvent::FindUseBufferChanged(on) => editor.set_find_use_buffer(on),
        UiEvent::FindAll => view.queue_command(Command::new("find_all")),
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
            view.queue_command(Command::with_args(
                "set_line_ending",
                serde_json::json!({ "type": next }),
            ));
        }

        UiEvent::TextEdited(document) => editor.note_text_edited(document),
        UiEvent::TextClicked(_) => editor.note_text_clicked(),
        UiEvent::TextDoubleClicked(document, point) => {
            if editor.open_result_in_tab(document, point) {
                view.focus_editor = true;
            }
        }
        UiEvent::SelectionChanged(document, selection) => {
            editor.blur_output_panel();
            view.focused_output_panel = None;
            view.output_panel_focus_requested = None;
            editor.sync_selection_from_view(document, selection)
        }
        UiEvent::VisibleRange(document, start, end) => {
            editor.sync_visible_range(document, start, end)
        }
        UiEvent::Unfold(document, point) => editor.unfold_at(document, point),
        UiEvent::ToggleFold(document, line) => editor.toggle_fold_at_line(document, line),
        UiEvent::ViewportScrolled(document) => editor.note_viewport_scrolled(document),
        UiEvent::ViewGeometry(document, geometry) => editor.sync_view_geometry(document, geometry),
        UiEvent::TextTyped(document, text) => editor.typed_in_view(document, &text),
        UiEvent::CompletionClicked(index) => {
            editor.commit_completion(Some(index));
            view.focus_editor = true;
        }
        UiEvent::TabPressed => {
            if !editor.tab_completion() {
                editor.run_text_command(TextCommand::InsertTab);
            }
        }
        UiEvent::ShiftTabPressed => editor.run_text_command(TextCommand::Unindent),
        UiEvent::EnterPressed => editor.run_text_command(TextCommand::NewlineAutoIndent),
        UiEvent::TypedAtSelections(text) => editor.insert_at_selections(&text),
        UiEvent::DeleteAtSelections { forward } => editor.delete_at_selections(forward),
        UiEvent::MoveCarets(motion) => editor.move_carets(motion),

        UiEvent::CloseDialogAnswered(choice) => {
            if editor.resolve_pending_close(choice) {
                view.exit_confirmed = true;
                view.request(ViewRequest::Exit);
            }
        }
        UiEvent::AboutClosed => view.show_about = false,
        UiEvent::LoadErrorsClosed => view.show_load_errors = false,
    }
}

/// [`UiEvent::SideBarSelect`]: the side bar's selection after a click on `path`.
fn select_in_side_bar(
    editor: &Editor,
    view: &mut ViewState,
    path: PathBuf,
    toggle: bool,
    range: bool,
) {
    if range {
        let anchor = view.side_bar_anchor.clone().unwrap_or_else(|| path.clone());
        let rows = presenters::side_bar::visible_paths(editor, view);
        let at = |p: &PathBuf| rows.iter().position(|row| row == p);
        if let (Some(from), Some(to)) = (at(&anchor), at(&path)) {
            let (from, to) = (from.min(to), from.max(to));
            view.side_bar_selection = rows[from..=to].to_vec();
            return;
        }
    }
    if toggle {
        if let Some(i) = view.side_bar_selection.iter().position(|p| *p == path) {
            view.side_bar_selection.remove(i);
        } else {
            view.side_bar_selection.push(path.clone());
        }
    } else {
        view.side_bar_selection = vec![path.clone()];
    }
    view.side_bar_anchor = Some(path);
}
