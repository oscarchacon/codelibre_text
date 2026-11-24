//! Use cases: the window's jump history (Goto > Jump Back / Jump Forward, `jump_back`,
//! `jump_forward`, `add_jump_record`), as Sublime Text 4's `Default/history_list.py` keeps it.
//! Every frame [`Editor::poll_jump_history`] looks at the active view, as that module's listener
//! hears `on_activated`, `on_modified` and `on_selection_modified`; the commands that ran since
//! (`note_command`) decide whether a change updates the last record or adds one (see
//! [`entities::jump_history`]). Each record's selections are a hidden region set of its view, so
//! they move with the text.

use std::time::Instant;

use entities::document::{Document, DocumentId, RegionSet};
use entities::jump_history::{
    JumpHistory, JumpRegions, PushKind, Selections, DRAG_SELECT, TEXT_MODIFIED,
};
use entities::selection::Selection;
use serde_json::Value;

use crate::editor::Editor;

/// `sublime.HIDDEN`: the records' region sets are not drawn.
const HIDDEN: u64 = 128;
/// A region set a plugin adds to say "do not record this selection" (Sublime's `symbol.py`).
const IGNORE_KEY: &str = "jump_ignore_selection";
/// More selections than this record only the first, as Sublime.
const MAX_SELECTIONS: usize = 10_000;

/// A window's history and what its active view was when last looked at.
#[derive(Debug, Default)]
pub(crate) struct JumpState {
    history: JumpHistory,
    seen: Option<Seen>,
    /// A command was noted since the last look.
    noted: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Seen {
    view: DocumentId,
    selections: Selections,
    change_count: u64,
}

/// The records' selections, as region sets of the window's views.
struct ViewRegions<'a>(&'a mut [Document]);

impl JumpRegions for ViewRegions<'_> {
    fn get(&self, view: DocumentId, key: &str) -> Selections {
        self.0
            .iter()
            .find(|d| d.id == view)
            .map(|d| d.regions(key))
            .unwrap_or_default()
    }

    fn set(&mut self, view: DocumentId, key: &str, selections: &Selections) {
        if let Some(doc) = self.0.iter_mut().find(|d| d.id == view) {
            doc.add_regions(RegionSet {
                key: key.to_string(),
                regions: selections.clone(),
                flags: HIDDEN,
                ..RegionSet::default()
            });
        }
    }

    fn erase(&mut self, view: DocumentId, key: &str) {
        if let Some(doc) = self.0.iter_mut().find(|d| d.id == view) {
            doc.erase_regions(key);
        }
    }
}

fn selections_of(doc: &Document) -> Selections {
    let all = doc.selections();
    let all = if all.len() > MAX_SELECTIONS {
        &all[..1]
    } else {
        &all[..]
    };
    all.iter().map(|s| (s.anchor, s.head)).collect()
}

impl Editor {
    /// A command is about to run in the current window (key bindings the text widget does itself
    /// included). A click in the text is [`DRAG_SELECT`].
    pub fn note_command(&mut self, name: &str) {
        let jumps = &mut self.state.jumps;
        jumps.history.record_command(name);
        jumps.noted = true;
        self.note_command_for_occurrences(name);
    }

    /// A click (or the start of a drag) in the text: the place it selects is a record of its own.
    pub fn note_text_clicked(&mut self) {
        self.note_command(DRAG_SELECT);
    }

    /// Every frame: records the active view's place when it was activated or its selections
    /// moved since the last look. Records of closed views go.
    pub fn poll_jump_history(&mut self) {
        self.poll_jump_history_at(Instant::now());
    }

    /// [`Self::poll_jump_history`] at a given time (what is within a second of what).
    pub fn poll_jump_history_at(&mut self, time: Instant) {
        self.forget_closed_views();
        let noted = std::mem::take(&mut self.state.jumps.noted);
        let Some(doc) = self.state.documents.get(self.state.active) else {
            self.state.jumps.seen = None;
            return;
        };
        let now = Seen {
            view: doc.id,
            selections: selections_of(doc),
            change_count: doc.change_count(),
        };
        let before = self.state.jumps.seen.replace(now.clone());
        let kind = match before {
            Some(before) if before.view == now.view => {
                if before.change_count != now.change_count {
                    self.state.jumps.history.record_command(TEXT_MODIFIED);
                } else if !noted && before.selections != now.selections {
                    // The widget's own caret keys: Sublime's `move` commands.
                    self.state.jumps.history.record_command("move");
                }
                if before.selections == now.selections {
                    return;
                }
                PushKind::Selection
            }
            _ => PushKind::Activation,
        };
        let command = self.state.jumps.history.current_command().to_string();
        let index = self.state.active;
        if let Some(doc) = self.state.documents.get_mut(index) {
            let ignored = doc.regions(IGNORE_KEY);
            if !ignored.is_empty() {
                doc.erase_regions(IGNORE_KEY);
                if ignored == now.selections {
                    return;
                }
            }
        }
        let state = &mut self.state;
        let mut regions = ViewRegions(&mut state.documents);
        if matches!(command.as_str(), "undo" | "redo" | "redo_or_repeat") {
            state
                .jumps
                .history
                .reorient(&regions, now.view, &now.selections);
        }
        state
            .jumps
            .history
            .push(&mut regions, now.view, now.selections, kind, time);
    }

    fn forget_closed_views(&mut self) {
        let state = &mut self.state;
        let mut closed: Vec<DocumentId> = state
            .jumps
            .history
            .records()
            .iter()
            .map(|r| r.view)
            .filter(|view| !state.documents.iter().any(|d| d.id == *view))
            .collect();
        closed.dedup();
        for view in closed {
            state.jumps.history.remove_view(view);
        }
    }

    /// `jump_back`: the place before the current one, in its view (focused).
    pub fn jump_back(&mut self) {
        self.jump(false);
    }

    /// `jump_forward`: the place after the current one.
    pub fn jump_forward(&mut self) {
        self.jump(true);
    }

    fn jump(&mut self, forward: bool) {
        let state = &mut self.state;
        let regions = ViewRegions(&mut state.documents);
        let place = if forward {
            state.jumps.history.jump_forward(&regions)
        } else {
            state.jumps.history.jump_back(&regions)
        };
        let Some((view, selections)) = place else {
            let message = if forward {
                self.tr("Already at the newest position")
            } else {
                self.tr("Already at the earliest position")
            }
            .to_string();
            self.notify(message);
            return;
        };
        self.activate_document(view);
        let Some(index) = self.index_of(view) else {
            return;
        };
        let doc = &mut self.state.documents[index];
        let mut all = selections.iter().map(|&(a, b)| Selection::new(a, b));
        let first = all.next().unwrap_or_default();
        doc.set_selections(first, all);
        doc.request_show(first.head, false);
        self.clear_status();
    }

    /// `add_jump_record {"selection": …}`: a plugin's place for the active view (a point, an
    /// `[a, b]` pair or a list of either), recorded without moving the selections. Unlike
    /// Sublime, it is never merged into the last record, which a quick second call would be.
    pub fn add_jump_record(&mut self, selection: Option<&Value>) -> Result<(), String> {
        let selections = parse_jump_selection(selection).ok_or_else(|| {
            "selection must be an int, 2-int list, or list of 2-int lists".to_string()
        })?;
        let Some(view) = self.active_document().map(|d| d.id) else {
            return Ok(());
        };
        let state = &mut self.state;
        let mut regions = ViewRegions(&mut state.documents);
        state.jumps.history.push(
            &mut regions,
            view,
            selections,
            PushKind::Explicit,
            Instant::now(),
        );
        Ok(())
    }

    /// The window's records, oldest first, with their selections (for tests and plugins).
    pub fn jump_records(&self) -> Vec<(DocumentId, Selections)> {
        self.state
            .jumps
            .history
            .records()
            .iter()
            .map(|record| {
                let selections = self
                    .state
                    .documents
                    .iter()
                    .find(|d| d.id == record.view)
                    .map(|d| d.regions(&record.key))
                    .unwrap_or_default();
                (record.view, selections)
            })
            .collect()
    }
}

/// `selection` of `add_jump_record`: an int, `[a, b]`, or a list of ints and `[a, b]` pairs.
fn parse_jump_selection(selection: Option<&Value>) -> Option<Selections> {
    let point = |v: &Value| v.as_u64().map(|p| p as usize);
    let pair = |v: &Value| match v.as_array().map(Vec::as_slice) {
        Some([a, b]) => Some((point(a)?, point(b)?)),
        _ => None,
    };
    match selection? {
        Value::Number(_) => point(selection?).map(|p| vec![(p, p)]),
        list @ Value::Array(items) => match pair(list) {
            Some(region) => Some(vec![region]),
            None => items
                .iter()
                .map(|item| pair(item).or_else(|| point(item).map(|p| (p, p))))
                .collect(),
        },
        _ => None,
    }
}
