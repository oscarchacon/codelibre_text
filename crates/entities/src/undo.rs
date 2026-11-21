//! Entities: a buffer's undo history (Sublime's `undo`, `redo`, `soft_undo`…), our own rather than
//! the text widget's, so the use cases undo at once (also while a plugin's event runs).
//!
//! Each edit is kept as what it changed, not as a copy of the text: where (a byte of the text as
//! it was just before it), what it took out and what it put in. Edits make steps, which `undo`
//! takes back whole:
//! - typing goes on in one step while each edit continues the last one (the next char after
//!   it, or a Backspace at its end); a line break, a paste (several chars at once), an edit
//!   elsewhere or a command starts another;
//! - a command's edits (one command, or a plugin's, however many `insert`s it makes) are one
//!   step, until the use cases close it ([`UndoHistory::close`]).

use crate::selection::Selection;

/// How many steps are kept; the oldest go first.
pub const MAX_STEPS: usize = 1000;

/// One change of the text: at byte `start`, `removed` became `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoEdit {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

impl UndoEdit {
    /// The byte after what it inserted, in the text just after it.
    fn end_after(&self) -> usize {
        self.start + self.inserted.len()
    }
}

/// Who made an edit: the text widget (the user typing, deleting, pasting) or a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Command,
}

/// The selections of a step: the primary one first.
pub type Selections = Vec<Selection>;

#[derive(Debug, Clone)]
struct Step {
    edits: Vec<UndoEdit>,
    kind: EditKind,
    /// Edits may still join it.
    open: bool,
    /// The selections before its first edit (what `undo` puts back) and after its last (`redo`).
    before: Selections,
    after: Selections,
}

impl Step {
    /// Whether a typing edit carries on from this step's last one.
    fn continued_by(&self, edit: &UndoEdit) -> bool {
        let Some(last) = self.edits.last() else {
            return false;
        };
        let end = last.end_after();
        let typed_one = edit.removed.is_empty() && edit.inserted.chars().count() == 1;
        let backspace = edit.inserted.is_empty() && edit.start + edit.removed.len() == end;
        let last_typed = last.inserted.chars().count() <= 1;
        last_typed
            && !edit.inserted.contains('\n')
            && ((typed_one && edit.start == end) || backspace)
    }
}

/// The steps done (the last is undone first) and those undone (redone in turn).
#[derive(Debug, Clone, Default)]
pub struct UndoHistory {
    done: Vec<Step>,
    undone: Vec<Step>,
}

impl UndoHistory {
    pub fn is_empty(&self) -> bool {
        self.done.is_empty() && self.undone.is_empty()
    }

    /// How many steps `undo` can take back.
    pub fn undo_count(&self) -> usize {
        self.done.len()
    }

    /// How many steps `redo` can do again.
    pub fn redo_count(&self) -> usize {
        self.undone.len()
    }

    /// Adds an edit, made with the selections `before`; `after` are the selections it leaves.
    /// A new edit forgets what was undone.
    pub fn record(
        &mut self,
        edit: UndoEdit,
        kind: EditKind,
        before: Selections,
        after: Selections,
    ) {
        self.undone.clear();
        if let Some(step) = self.done.last_mut().filter(|s| s.open && s.kind == kind) {
            if kind == EditKind::Command || step.continued_by(&edit) {
                step.edits.push(edit);
                step.after = after;
                return;
            }
        }
        self.close();
        // Several chars typed at once (a paste, an IME's text) are a step of their own.
        let open = kind == EditKind::Command || edit.inserted.chars().count() <= 1;
        self.done.push(Step {
            edits: vec![edit],
            kind,
            open,
            before,
            after,
        });
        if self.done.len() > MAX_STEPS {
            self.done.remove(0);
        }
    }

    /// The selections the last step leaves, once its command set them.
    pub fn set_after(&mut self, after: Selections) {
        if let Some(step) = self.done.last_mut().filter(|s| s.open) {
            step.after = after;
        }
    }

    /// Whether the last step is still open and leaves the primary selection at `selection` (the
    /// caret after what was just typed).
    pub fn leaves(&self, selection: Selection) -> bool {
        self.done
            .last()
            .is_some_and(|s| s.open && s.after.first() == Some(&selection))
    }

    /// No further edit joins the last step.
    pub fn close(&mut self) {
        if let Some(step) = self.done.last_mut() {
            step.open = false;
        }
    }

    /// Closes the last step when a command made it: what comes next is another command's.
    pub fn close_command(&mut self) {
        if let Some(step) = self.done.last_mut().filter(|s| s.kind == EditKind::Command) {
            step.open = false;
        }
    }

    /// Takes back the last step: the edits that undo it (to apply in order) and the selections
    /// to put back. `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<(Vec<UndoEdit>, Selections)> {
        let mut step = self.done.pop()?;
        step.open = false;
        let reverse = step
            .edits
            .iter()
            .rev()
            .map(|e| UndoEdit {
                start: e.start,
                removed: e.inserted.clone(),
                inserted: e.removed.clone(),
            })
            .collect();
        let selections = step.before.clone();
        self.undone.push(step);
        Some((reverse, selections))
    }

    /// Does again the last step undone: its edits (in order) and the selections it left.
    pub fn redo(&mut self) -> Option<(Vec<UndoEdit>, Selections)> {
        let step = self.undone.pop()?;
        let edits = step.edits.clone();
        let selections = step.after.clone();
        self.done.push(step);
        Some((edits, selections))
    }
}
