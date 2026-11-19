//! Tests for the jump history rules (`entities::jump_history`), Sublime's `history_list.py`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use entities::document::DocumentId;
use entities::jump_history::{
    JumpHistory, JumpRegions, PushKind, Selections, DRAG_SELECT, LIST_LIMIT, TEXT_MODIFIED,
};

/// Region sets by view and key, as documents keep them.
#[derive(Default)]
struct Regions(HashMap<(DocumentId, String), Selections>);

impl JumpRegions for Regions {
    fn get(&self, view: DocumentId, key: &str) -> Selections {
        self.0
            .get(&(view, key.to_string()))
            .cloned()
            .unwrap_or_default()
    }
    fn set(&mut self, view: DocumentId, key: &str, selections: &Selections) {
        self.0.insert((view, key.to_string()), selections.clone());
    }
    fn erase(&mut self, view: DocumentId, key: &str) {
        self.0.remove(&(view, key.to_string()));
    }
}

fn caret(p: usize) -> Selections {
    vec![(p, p)]
}

/// Pushes carets in `view`, each after `command`, two seconds apart.
fn places(history: &mut JumpHistory, regions: &mut Regions, view: DocumentId, carets: &[usize]) {
    let mut now = Instant::now();
    for &p in carets {
        history.record_command("goto_line");
        history.push(regions, view, caret(p), PushKind::Selection, now);
        now += Duration::from_secs(2);
    }
}

#[test]
fn jump_back_and_forward_walk_the_records() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    places(&mut history, &mut regions, 1, &[0, 10, 20]);
    assert_eq!(history.len(), 3);
    assert_eq!(history.jump_back(&regions), Some((1, caret(10))));
    assert_eq!(history.jump_back(&regions), Some((1, caret(0))));
    assert_eq!(history.jump_back(&regions), None);
    assert_eq!(history.jump_forward(&regions), Some((1, caret(10))));
    assert_eq!(history.jump_forward(&regions), Some((1, caret(20))));
    assert_eq!(history.jump_forward(&regions), None);
}

#[test]
fn a_new_place_after_jumping_back_drops_the_records_ahead() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    places(&mut history, &mut regions, 1, &[0, 10, 20]);
    history.record_command("jump_back");
    history.jump_back(&regions);
    // The jump's own selection change records nothing.
    let now = Instant::now() + Duration::from_secs(10);
    history.push(&mut regions, 1, caret(10), PushKind::Selection, now);
    assert_eq!(history.len(), 3);
    history.record_command("goto_line");
    history.push(&mut regions, 1, caret(5), PushKind::Selection, now);
    assert_eq!(history.len(), 3, "20 went, 5 came");
    assert_eq!(history.current_item(), -1);
    assert_eq!(
        regions.0.len(),
        3,
        "the dropped record's region set is erased"
    );
    assert_eq!(history.jump_back(&regions), Some((1, caret(10))));
}

#[test]
fn a_command_repeated_within_a_second_updates_its_record() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    let now = Instant::now();
    history.record_command("move");
    history.push(&mut regions, 1, caret(1), PushKind::Selection, now);
    history.record_command("move");
    history.push(
        &mut regions,
        1,
        caret(2),
        PushKind::Selection,
        now + Duration::from_millis(500),
    );
    assert_eq!(history.len(), 1);
    // After a pause, a record of its own.
    history.record_command("move");
    history.push(
        &mut regions,
        1,
        caret(3),
        PushKind::Selection,
        now + Duration::from_secs(3),
    );
    assert_eq!(history.len(), 2);
    // Typing then moving is another command.
    history.record_command(TEXT_MODIFIED);
    history.push(
        &mut regions,
        1,
        caret(4),
        PushKind::Selection,
        now + Duration::from_millis(3100),
    );
    assert_eq!(history.len(), 3);
}

#[test]
fn each_click_starts_a_record_of_its_own() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    let now = Instant::now();
    for (i, p) in [5, 9].into_iter().enumerate() {
        history.record_command(DRAG_SELECT);
        let at = now + Duration::from_millis(10 * i as u64);
        history.push(&mut regions, 1, caret(p), PushKind::Selection, at);
    }
    assert_eq!(history.len(), 2, "two quick clicks are two places");
}

#[test]
fn a_change_right_after_an_activation_updates_the_activation_s_record() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    let now = Instant::now();
    history.push(&mut regions, 1, caret(0), PushKind::Activation, now);
    history.record_command("goto_definition");
    history.push(
        &mut regions,
        2,
        caret(7),
        PushKind::Activation,
        now + Duration::from_secs(2),
    );
    history.push(
        &mut regions,
        2,
        caret(40),
        PushKind::Selection,
        now + Duration::from_millis(2050),
    );
    assert_eq!(history.len(), 2);
    assert_eq!(history.jump_back(&regions), Some((1, caret(0))));
    assert_eq!(history.jump_forward(&regions), Some((2, caret(40))));
}

#[test]
fn undo_records_nothing_and_the_same_place_is_not_recorded_twice() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    places(&mut history, &mut regions, 1, &[0, 10]);
    let later = Instant::now() + Duration::from_secs(10);
    history.record_command("goto_line");
    history.push(&mut regions, 1, caret(10), PushKind::Selection, later);
    assert_eq!(history.len(), 2);
    history.record_command("undo");
    history.record_command(TEXT_MODIFIED);
    assert_eq!(history.current_command(), "undo", "the edit is the undo's");
    history.push(&mut regions, 1, caret(3), PushKind::Selection, later);
    assert_eq!(history.len(), 2);
}

#[test]
fn an_explicit_record_is_never_merged() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    let now = Instant::now();
    history.record_command("add_jump_record");
    history.push(&mut regions, 1, caret(1), PushKind::Explicit, now);
    history.push(&mut regions, 1, caret(2), PushKind::Explicit, now);
    history.push(&mut regions, 1, caret(2), PushKind::Explicit, now);
    assert_eq!(history.len(), 2);
}

#[test]
fn a_closed_view_s_records_go_and_the_current_place_stays() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    places(&mut history, &mut regions, 1, &[0]);
    places(&mut history, &mut regions, 2, &[5]);
    places(&mut history, &mut regions, 1, &[10]);
    places(&mut history, &mut regions, 2, &[15]);
    assert_eq!(history.jump_back(&regions), Some((1, caret(10))));
    history.remove_view(2);
    assert_eq!(history.len(), 2);
    assert_eq!(history.current_item(), -1, "still at 10, now the newest");
    assert_eq!(history.jump_back(&regions), Some((1, caret(0))));
    history.remove_view(1);
    assert!(history.is_empty());
    assert_eq!(history.jump_back(&regions), None);
}

#[test]
fn records_beyond_the_limit_go_a_third_at_a_time() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    let carets: Vec<usize> = (0..=LIST_LIMIT).collect();
    places(&mut history, &mut regions, 1, &carets);
    assert_eq!(history.len(), LIST_LIMIT + 1 - LIST_LIMIT / 3);
    assert_eq!(regions.0.len(), history.len());
}

#[test]
fn after_an_undo_the_record_of_the_view_s_selections_is_the_current_one() {
    let (mut history, mut regions) = (JumpHistory::default(), Regions::default());
    places(&mut history, &mut regions, 1, &[0, 10, 20]);
    history.reorient(&regions, 1, &caret(10));
    assert_eq!(history.current_item(), -2);
    assert_eq!(history.jump_back(&regions), Some((1, caret(0))));
}
