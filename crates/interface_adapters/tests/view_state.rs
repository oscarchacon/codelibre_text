//! Tests for `view_state` (presentation-only state).

use entities::command::Command;
use interface_adapters::view_state::{QuickPanel, ViewRequest, ViewState, WidgetEdit};
use use_cases::search::SearchKind;

#[test]
fn a_new_view_shows_the_bars_and_focuses_the_text() {
    let view = ViewState::default();
    assert!(view.show_side_bar && view.show_status_bar && view.focus_editor);
    assert!(!view.fullscreen && !view.show_about && !view.show_load_errors && !view.exit_confirmed);
    assert!(view.quick_panel.is_none() && !view.find_panel.visible);
}

#[test]
fn commands_are_taken_in_order_once() {
    let mut view = ViewState::default();
    view.queue_command(Command::new("a"));
    view.queue_command(Command::new("b"));
    let names: Vec<String> = view.take_commands().into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["a", "b"]);
    assert!(view.take_commands().is_empty());
}

#[test]
fn requests_are_taken_in_order_once() {
    let mut view = ViewState::default();
    view.request(ViewRequest::WidgetEdit(WidgetEdit::Undo));
    view.request(ViewRequest::CloseWindow);
    assert_eq!(view.take_requests(), vec![ViewRequest::WidgetEdit(WidgetEdit::Undo), ViewRequest::CloseWindow]);
    assert!(view.take_requests().is_empty());
}

#[test]
fn the_layout_round_trips_through_the_session() {
    let mut view = ViewState::default();
    view.show_side_bar = false;
    let layout = view.layout();
    assert_eq!(layout, vec![("side_bar".to_string(), false), ("status_bar".to_string(), true)]);

    let mut restored = ViewState::default();
    restored.apply_layout(&layout);
    assert!(!restored.show_side_bar && restored.show_status_bar);
}

#[test]
fn unknown_layout_flags_are_ignored() {
    let mut view = ViewState::default();
    view.apply_layout(&[("minimap".into(), false)]);
    assert!(view.show_side_bar && view.show_status_bar);
}

#[test]
fn a_new_quick_panel_asks_for_focus() {
    let panel = QuickPanel::new(SearchKind::Goto, ":".into());
    assert!(panel.focus_requested);
    assert_eq!(panel.query, ":");
    assert_eq!(panel.selected, 0);
    assert!(panel.results.is_empty() && panel.results_query.is_none() && panel.color_scheme_before.is_none());
}
