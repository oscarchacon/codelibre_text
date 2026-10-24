//! Tests for `controllers::ui_controller` (what the user does in the views).

use std::path::PathBuf;

use entities::command::Command;
use entities::find::FindOptions;
use entities::selection::Selection;
use interface_adapters::controllers::ui_controller::{handle, UiEvent};
use interface_adapters::view_state::{QuickPanel, ViewRequest, ViewState};
use serde_json::json;
use test_support::{active_text, open, World};
use use_cases::editing::TextCommand;
use use_cases::editor::{CloseChoice, PendingClose};
use use_cases::search::{Candidate, CandidateTarget, SearchKind};
use use_cases::Editor;

fn queued(view: &mut ViewState) -> Vec<Command> {
    view.take_commands().into_iter().collect()
}

fn candidate(caption: &str, target: CandidateTarget) -> Candidate {
    Candidate { caption: caption.into(), detail: String::new(), hint: String::new(), target }
}

fn editor_with_text(world: &World, text: &str) -> Editor {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    editor
}

// ----- menus, tabs and side bar --------------------------------------------------------------------

#[test]
fn menu_commands_are_queued() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    handle(&mut editor, &mut view, UiEvent::RunCommand(Command::new("save")));
    assert_eq!(queued(&mut view), vec![Command::new("save")]);
}

#[test]
fn tab_events() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    handle(&mut editor, &mut view, UiEvent::NewTab);
    assert_eq!(editor.state().documents.len(), 2);
    view.focus_editor = false;
    handle(&mut editor, &mut view, UiEvent::ActivateTab(0));
    assert_eq!(editor.state().active, 0);
    assert!(view.focus_editor);
    handle(&mut editor, &mut view, UiEvent::CloseTab(1));
    assert_eq!(editor.state().documents.len(), 1);
    handle(&mut editor, &mut view, UiEvent::CloseTab(9));
    assert_eq!(editor.state().documents.len(), 1, "unknown tabs are ignored");
}

#[test]
fn closing_a_dirty_tab_asks_first() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "draft");
    let mut view = ViewState::default();
    handle(&mut editor, &mut view, UiEvent::CloseTab(0));
    assert!(matches!(editor.state().pending_close, Some(PendingClose::Documents(_))));
}

#[test]
fn side_bar_events() {
    let world = World::new().with_dir("/p").with_file("/p/a.txt", "a");
    let mut editor = world.editor_opening(&["/p"]);
    let mut view = ViewState::default();
    let folder = PathBuf::from("/p");

    handle(&mut editor, &mut view, UiEvent::ToggleFolder(folder.clone()));
    assert!(view.expanded_folders.contains(&folder));
    handle(&mut editor, &mut view, UiEvent::ToggleFolder(folder.clone()));
    assert!(!view.expanded_folders.contains(&folder));

    handle(&mut editor, &mut view, UiEvent::OpenFile(PathBuf::from("/p/a.txt")));
    assert_eq!(active_text(&editor), "a");

    handle(&mut editor, &mut view, UiEvent::RemoveFolder(folder));
    assert!(editor.state().folders.is_empty());
}

// ----- quick panel ---------------------------------------------------------------------------------

#[test]
fn typing_in_the_quick_panel_resets_the_selection() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::Commands, String::new());
    panel.selected = 3;
    view.quick_panel = Some(panel);
    handle(&mut editor, &mut view, UiEvent::QuickPanelQueryChanged("sav".into()));
    let panel = view.quick_panel.as_ref().unwrap();
    assert_eq!(panel.query, "sav");
    assert_eq!(panel.selected, 0);
}

#[test]
fn typing_a_line_number_in_goto_moves_the_caret_live() {
    let world = World::new().with_file("/p/a.txt", "a\nb\nc");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let mut view = ViewState::default();
    view.quick_panel = Some(QuickPanel::new(SearchKind::Goto, ":".into()));
    handle(&mut editor, &mut view, UiEvent::QuickPanelQueryChanged(":3".into()));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(4));
    assert!(view.quick_panel.is_some(), "still open until Enter");
}

#[test]
fn moving_through_color_schemes_previews_them() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::ColorSchemes, String::new());
    panel.results = vec![candidate("Monokai", CandidateTarget::ColorScheme("Monokai".into())), candidate("Daylight", CandidateTarget::ColorScheme("Daylight".into()))];
    view.quick_panel = Some(panel);
    handle(&mut editor, &mut view, UiEvent::QuickPanelHighlighted(1));
    assert_eq!(view.quick_panel.as_ref().unwrap().selected, 1);
    assert_eq!(editor.color_scheme_name(), "Daylight");
}

#[test]
fn highlighting_other_results_does_not_preview() {
    let world = World::new().with_file("/p/a.txt", "a\nb");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::Goto, String::new());
    panel.results = vec![candidate("x", CandidateTarget::Line(1))];
    view.quick_panel = Some(panel);
    handle(&mut editor, &mut view, UiEvent::QuickPanelHighlighted(0));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(0));
}

#[test]
fn accepting_a_palette_entry_queues_its_command_and_closes_the_panel() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::Commands, String::new());
    panel.results = vec![candidate("File: Save", CandidateTarget::Command(Command::new("save")))];
    view.quick_panel = Some(panel);
    view.focus_editor = false;
    handle(&mut editor, &mut view, UiEvent::QuickPanelAccepted(0));
    assert!(view.quick_panel.is_none());
    assert!(view.focus_editor);
    assert_eq!(queued(&mut view), vec![Command::new("save")]);
}

#[test]
fn accepting_a_file_opens_it() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::Goto, String::new());
    panel.results = vec![candidate("a.txt", CandidateTarget::OpenFile(PathBuf::from("/p/a.txt")))];
    view.quick_panel = Some(panel);
    handle(&mut editor, &mut view, UiEvent::QuickPanelAccepted(0));
    assert_eq!(active_text(&editor), "a");
}

#[test]
fn dismissing_the_color_scheme_picker_restores_the_scheme() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let mut panel = QuickPanel::new(SearchKind::ColorSchemes, String::new());
    panel.color_scheme_before = Some("Monokai".into());
    view.quick_panel = Some(panel);
    editor.preview_color_scheme("Daylight");
    handle(&mut editor, &mut view, UiEvent::QuickPanelDismissed);
    assert!(view.quick_panel.is_none());
    assert_eq!(editor.color_scheme_name(), "Monokai");
}

// ----- find panel ----------------------------------------------------------------------------------

#[test]
fn find_panel_edits_update_the_search() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let options = FindOptions { query: "x".into(), regex: true, ..Default::default() };
    handle(&mut editor, &mut view, UiEvent::FindOptionsChanged(options.clone()));
    handle(&mut editor, &mut view, UiEvent::ReplacementChanged("y".into()));
    assert_eq!(editor.state().find, options);
    assert_eq!(editor.state().replacement, "y");
}

#[test]
fn find_panel_buttons_queue_sublime_commands() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    for event in [UiEvent::FindNext, UiEvent::FindPrevious, UiEvent::ReplaceNext, UiEvent::ReplaceAll, UiEvent::CloseFindPanel] {
        handle(&mut editor, &mut view, event);
    }
    let names: Vec<String> = queued(&mut view).into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["find_next", "find_prev", "replace_next", "replace_all", "hide_panel"]);
}

// ----- status bar ----------------------------------------------------------------------------------

#[test]
fn status_bar_clicks() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    handle(&mut editor, &mut view, UiEvent::SyntaxClicked);
    handle(&mut editor, &mut view, UiEvent::IndentationClicked);
    handle(&mut editor, &mut view, UiEvent::LineEndingClicked);
    assert_eq!(
        queued(&mut view),
        vec![
            Command::with_args("show_overlay", json!({ "overlay": "command_palette", "text": "Set Syntax: " })),
            Command::with_args("toggle_setting", json!({ "setting": "translate_tabs_to_spaces" })),
            Command::with_args("set_line_ending", json!({ "type": "windows" })),
        ]
    );
}

#[test]
fn the_line_ending_click_toggles_back_to_unix() {
    let world = World::new();
    let mut editor = world.editor();
    editor.set_line_ending(entities::document::LineEnding::Windows);
    let mut view = ViewState::default();
    handle(&mut editor, &mut view, UiEvent::LineEndingClicked);
    assert_eq!(queued(&mut view)[0].str_arg("type"), "unix");
}

// ----- the text area -------------------------------------------------------------------------------

#[test]
fn caret_moves_in_the_text_widget_are_recorded() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "hello");
    let mut view = ViewState::default();
    let id = editor.active_document().unwrap().id;
    handle(&mut editor, &mut view, UiEvent::SelectionChanged(id, Selection::new(1, 4)));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(1, 4));
}

#[test]
fn tab_shift_tab_and_enter_use_sublime_indentation() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "{}");
    let mut view = ViewState::default();
    let id = editor.active_document().unwrap().id;
    handle(&mut editor, &mut view, UiEvent::SelectionChanged(id, Selection::caret(1)));
    handle(&mut editor, &mut view, UiEvent::EnterPressed);
    assert_eq!(active_text(&editor), "{\n\t\n}");
    handle(&mut editor, &mut view, UiEvent::ShiftTabPressed);
    assert_eq!(active_text(&editor), "{\n\n}");
    handle(&mut editor, &mut view, UiEvent::TabPressed);
    assert_eq!(active_text(&editor), "{\n\t\n}");
}

// ----- dialogs -------------------------------------------------------------------------------------

#[test]
fn answering_the_exit_question_closes_the_window() {
    let world = World::new().with_preferences(json!({ "hot_exit": false }));
    let mut editor = editor_with_text(&world, "unsaved");
    let mut view = ViewState::default();
    editor.request_exit(view.layout());
    handle(&mut editor, &mut view, UiEvent::CloseDialogAnswered(CloseChoice::DontSave));
    assert!(view.exit_confirmed);
    assert_eq!(view.take_requests(), vec![ViewRequest::CloseWindow]);
}

#[test]
fn cancelling_a_close_question_keeps_everything() {
    let world = World::new();
    let mut editor = editor_with_text(&world, "unsaved");
    let mut view = ViewState::default();
    editor.close_active_document();
    handle(&mut editor, &mut view, UiEvent::CloseDialogAnswered(CloseChoice::Cancel));
    assert!(!view.exit_confirmed);
    assert!(view.take_requests().is_empty());
    assert_eq!(editor.state().documents.len(), 1);
}

#[test]
fn closing_the_about_and_errors_windows() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    view.show_about = true;
    view.show_load_errors = true;
    handle(&mut editor, &mut view, UiEvent::AboutClosed);
    handle(&mut editor, &mut view, UiEvent::LoadErrorsClosed);
    assert!(!view.show_about && !view.show_load_errors);
}
