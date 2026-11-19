//! The jump history of a window (Goto > Jump Back / Jump Forward): where the user's selections
//! were, so they can go back to them. The rules are those of Sublime Text 4's
//! `Default/history_list.py`: a record per place, a command repeated within a second updates
//! its record instead of adding one, a view activated and then moved at once makes one record,
//! undo and the jumps themselves record nothing, and a click starts a record of its own.
//!
//! A record's selections are not kept here: they live in the view as a hidden region set (the
//! record's `key`), so they move with the text as it is edited. [`JumpRegions`] reads and writes
//! them.

use std::time::{Duration, Instant};

use crate::document::DocumentId;

/// At most this many records (Sublime's `LIST_LIMIT`).
pub const LIST_LIMIT: usize = 120;
/// A command repeated within this time updates its record (`TIME_BETWEEN_RECORDING`).
const TIME_BETWEEN_RECORDING: Duration = Duration::from_secs(1);
/// A selection change this soon after an activation updates its record (`TIME_AFTER_ACTIVATE`).
const TIME_AFTER_ACTIVATE: Duration = Duration::from_millis(100);

/// Undo and redo: what they do to the selections is not recorded.
const UNDO_COMMANDS: &[&str] = &["soft_undo", "soft_redo", "undo", "redo_or_repeat", "redo"];
/// The command a text edit counts as (Sublime's `on_modified`).
pub const TEXT_MODIFIED: &str = ":text_modified";
/// A click or a drag in the text.
pub const DRAG_SELECT: &str = "drag_select";

/// `(a, b)` char positions, one pair per selection, in the view's order.
pub type Selections = Vec<(usize, usize)>;

/// Where a record's selections are kept: a region set of its view, named by the record's key.
pub trait JumpRegions {
    /// The regions of `key` in `view` (empty when the view or the set is gone).
    fn get(&self, view: DocumentId, key: &str) -> Selections;
    fn set(&mut self, view: DocumentId, key: &str, selections: &Selections);
    fn erase(&mut self, view: DocumentId, key: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpRecord {
    /// The region set of `view` that holds the selections.
    pub key: String,
    pub view: DocumentId,
}

/// The records of one window, oldest first, and where the user is among them.
#[derive(Debug, Clone)]
pub struct JumpHistory {
    records: Vec<JumpRecord>,
    /// The command the user ran last (or [`TEXT_MODIFIED`]).
    current_command: String,
    /// `current_command` differs from the one before it.
    different_command: bool,
    last_change: Option<Instant>,
    /// The last change to the records came from an activation.
    last_was_activation: bool,
    /// Where the user is, counted from the newest record: -1 is the newest.
    current_item: isize,
    key_counter: usize,
}

impl Default for JumpHistory {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            current_command: String::new(),
            different_command: false,
            last_change: None,
            last_was_activation: false,
            current_item: -1,
            key_counter: 0,
        }
    }
}

/// What a push records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushKind {
    /// The view's selections changed.
    Selection,
    /// The view was activated.
    Activation,
    /// A plugin asked for this place (`add_jump_record`): always a record of its own, unless
    /// it is the place the last record already holds.
    Explicit,
}

impl JumpHistory {
    pub fn records(&self) -> &[JumpRecord] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Where the user is, counted from the newest record (-1).
    pub fn current_item(&self) -> isize {
        self.current_item
    }

    fn at(&self, item: isize) -> &JumpRecord {
        &self.records[self.position(item)]
    }

    fn position(&self, item: isize) -> usize {
        (self.records.len() as isize + item) as usize
    }

    fn oldest(&self) -> isize {
        -(self.records.len() as isize)
    }

    /// A command is about to run in the window ([`TEXT_MODIFIED`] for an edit): decides whether
    /// the selection changes that follow update the last record or make a new one.
    pub fn record_command(&mut self, command: &str) {
        self.different_command = self.current_command != command;
        // What undo and redo change is theirs: the edit does not make it a new command.
        if command == TEXT_MODIFIED && UNDO_COMMANDS.contains(&self.current_command.as_str()) {
            return;
        }
        self.current_command = command.to_string();
    }

    /// The command noted last.
    pub fn current_command(&self) -> &str {
        &self.current_command
    }

    /// Records `selections` of `view`, unless that would confuse the user: after a jump or an
    /// undo nothing is recorded, the same place is not recorded twice, and a quick repetition
    /// of a command (or a change right after an activation) updates the last record.
    pub fn push(
        &mut self,
        regions: &mut impl JumpRegions,
        view: DocumentId,
        selections: Selections,
        kind: PushKind,
        now: Instant,
    ) {
        let command = self.current_command.as_str();
        if kind != PushKind::Explicit
            && (command == "jump_back"
                || command == "jump_forward"
                || UNDO_COMMANDS.contains(&command))
        {
            self.current_command = ":empty".to_string();
            return;
        }
        if selections.is_empty() {
            return;
        }
        if !self.records.is_empty() {
            // The newest record that still has selections (a view's edits may have emptied some).
            let mut item = self.current_item;
            let mut previous = regions.get(self.at(item).view, &self.at(item).key);
            while previous.is_empty() && item > self.oldest() {
                item -= 1;
                previous = regions.get(self.at(item).view, &self.at(item).key);
            }
            let record = self.at(item).clone();
            if !previous.is_empty() && record.view == view && previous == selections {
                return;
            }
            let since = self
                .last_change
                .map_or(Duration::MAX, |t| now.saturating_duration_since(t));
            let just_activated = since <= TIME_AFTER_ACTIVATE && self.last_was_activation;
            let repeated =
                since <= TIME_BETWEEN_RECORDING && !self.different_command && record.view == view;
            if kind != PushKind::Explicit && (just_activated || repeated) {
                regions.set(view, &record.key, &selections);
                if record.view != view {
                    regions.erase(record.view, &record.key);
                    let position = self.position(item);
                    self.records[position].view = view;
                }
                self.last_change = Some(now);
                self.last_was_activation = !just_activated && kind == PushKind::Activation;
                return;
            }
        }
        // A new place after jumping back: the records ahead of it go.
        if self.current_item != -1 {
            let keep = self.position(self.current_item) + 1;
            for record in self.records.drain(keep..) {
                regions.erase(record.view, &record.key);
            }
            self.current_item = -1;
        }
        let key = self.generate_key();
        regions.set(view, &key, &selections);
        self.records.push(JumpRecord { key, view });
        if self.records.len() > LIST_LIMIT {
            // A third at a time, so this does not happen at every push.
            for record in self.records.drain(..LIST_LIMIT / 3) {
                regions.erase(record.view, &record.key);
            }
        }
        self.last_change = Some(now);
        self.last_was_activation = kind == PushKind::Activation;
        // The start of a drag gets a record of its own; what the drag selects then updates it.
        if self.current_command == "post:drag_select" {
            self.different_command = false;
        } else if self.current_command == DRAG_SELECT {
            self.current_command = "post:drag_select".to_string();
            self.different_command = true;
        }
    }

    /// The place before the current one (a different view or other selections), which becomes
    /// the current one; `None` at the oldest.
    pub fn jump_back(&mut self, regions: &impl JumpRegions) -> Option<(DocumentId, Selections)> {
        self.jump(regions, false)
    }

    /// The place after the current one; `None` at the newest.
    pub fn jump_forward(&mut self, regions: &impl JumpRegions) -> Option<(DocumentId, Selections)> {
        self.jump(regions, true)
    }

    fn jump(
        &mut self,
        regions: &impl JumpRegions,
        forward: bool,
    ) -> Option<(DocumentId, Selections)> {
        if self.records.is_empty() {
            return None;
        }
        let mut item = self.current_item;
        let current = self.at(item).clone();
        let mut current_selections = regions.get(current.view, &current.key);
        let end = if forward { -1 } else { self.oldest() };
        loop {
            if item == end {
                return None;
            }
            item += if forward { 1 } else { -1 };
            let record = self.at(item).clone();
            let selections = regions.get(record.view, &record.key);
            if !selections.is_empty()
                && (record.view != current.view || selections != current_selections)
            {
                if current_selections.is_empty() {
                    current_selections = selections;
                } else {
                    self.current_item = item;
                    return Some((record.view, selections));
                }
            }
        }
    }

    /// After an undo or redo: the record of the view's selections, if there is one, becomes the
    /// current one.
    pub fn reorient(
        &mut self,
        regions: &impl JumpRegions,
        view: DocumentId,
        selections: &Selections,
    ) {
        let mut start = -1;
        while start >= self.oldest() {
            let mut item = start;
            let mut found = regions.get(self.at(item).view, &self.at(item).key);
            while found.is_empty() && item > self.oldest() {
                item -= 1;
                found = regions.get(self.at(item).view, &self.at(item).key);
            }
            if !found.is_empty() && self.at(item).view == view && &found == selections {
                self.current_item = item;
                return;
            }
            start -= 1;
        }
    }

    /// A view closed: its records go (the current place stays where it was among the others).
    pub fn remove_view(&mut self, view: DocumentId) {
        if self.records.is_empty() {
            return;
        }
        let current = self.position(self.current_item.max(self.oldest()));
        let mut removed_after = 0;
        let mut index = 0;
        self.records.retain(|record| {
            let gone = record.view == view;
            if gone && index > current {
                removed_after += 1;
            }
            index += 1;
            !gone
        });
        self.current_item = (self.current_item + removed_after).clamp(self.oldest().min(-1), -1);
    }

    fn generate_key(&mut self) -> String {
        // Enough keys for five times the limit, as Sublime.
        self.key_counter = (self.key_counter + 1) % (LIST_LIMIT * 5);
        format!("jump_key_{:#x}", self.key_counter)
    }
}
