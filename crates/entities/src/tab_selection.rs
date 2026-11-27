//! Sublime 4's multi-selected tabs: a group of the window may have several tabs selected, shown
//! side by side, one of them focused (the group's active view). Each group has its own.
//!
//! The rules here work on positions among a group's tabs. They were checked against Sublime
//! 4215 (a probe plugin in a portable copy running the commands with several selections):
//! - `select_to_left` / `select_to_right` add the tab next to the leftmost / rightmost selected
//!   one, and focus it (nothing when there is none);
//! - `unselect_to_left` / `unselect_to_right` take away the selected tabs on that side of the
//!   focused one, `unselect_others` all but it;
//! - `unselect_current` takes the focused one away: the focus goes to the next selected tab to
//!   its right, else to its left (nothing when it is the only one); closing the focused tab of a
//!   selection does the same;
//! - `focus_to_left` / `focus_to_right` move the focus among the selected tabs (no wrapping),
//!   `focus_by_index` to the selected tab at that index (nothing beyond them).

/// The selected tabs of a group: their positions among its tabs, in order, and the focused one
/// (always one of them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSelection {
    selected: Vec<usize>,
    focused: usize,
}

/// The commands of Selection > Tab Selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabSelectionCommand {
    UnselectOthers,
    UnselectCurrent,
    UnselectToLeft,
    UnselectToRight,
    SelectToLeft,
    SelectToRight,
    FocusToLeft,
    FocusToRight,
    FocusByIndex(usize),
}

impl TabSelectionCommand {
    /// The command of that name (`focus_by_index` with its `index`).
    pub fn from_name(name: &str, index: usize) -> Option<Self> {
        Some(match name {
            "unselect_others" => Self::UnselectOthers,
            "unselect_current" => Self::UnselectCurrent,
            "unselect_to_left" => Self::UnselectToLeft,
            "unselect_to_right" => Self::UnselectToRight,
            "select_to_left" => Self::SelectToLeft,
            "select_to_right" => Self::SelectToRight,
            "focus_to_left" => Self::FocusToLeft,
            "focus_to_right" => Self::FocusToRight,
            "focus_by_index" => Self::FocusByIndex(index),
            _ => return None,
        })
    }
}

impl TabSelection {
    /// Only the tab at `focused`.
    pub fn single(focused: usize) -> Self {
        Self {
            selected: vec![focused],
            focused,
        }
    }

    /// `selected` (in any order, repeated or not) and `focused`, which is selected too.
    pub fn new(selected: impl IntoIterator<Item = usize>, focused: usize) -> Self {
        let mut selected: Vec<usize> = selected.into_iter().collect();
        selected.push(focused);
        selected.sort_unstable();
        selected.dedup();
        Self { selected, focused }
    }

    /// Positions of the selected tabs, in order.
    pub fn selected(&self) -> &[usize] {
        &self.selected
    }

    pub fn focused(&self) -> usize {
        self.focused
    }

    /// Several tabs are selected (the `group_has_multiselect` context).
    pub fn is_multiple(&self) -> bool {
        self.selected.len() > 1
    }

    pub fn contains(&self, position: usize) -> bool {
        self.selected.binary_search(&position).is_ok()
    }

    /// The selection after `command`, in a group of `tab_count` tabs.
    pub fn apply(&self, command: TabSelectionCommand, tab_count: usize) -> Self {
        let focused = self.focused;
        match command {
            TabSelectionCommand::UnselectOthers => Self::single(focused),
            TabSelectionCommand::UnselectCurrent => self.without_focused().unwrap_or(self.clone()),
            TabSelectionCommand::UnselectToLeft => self.keep(|p| p >= focused, focused),
            TabSelectionCommand::UnselectToRight => self.keep(|p| p <= focused, focused),
            TabSelectionCommand::SelectToLeft => match self.selected[0].checked_sub(1) {
                Some(left) => Self::new(self.selected.iter().copied(), left),
                None => self.clone(),
            },
            TabSelectionCommand::SelectToRight => {
                let right = self.selected[self.selected.len() - 1] + 1;
                if right < tab_count {
                    Self::new(self.selected.iter().copied(), right)
                } else {
                    self.clone()
                }
            }
            TabSelectionCommand::FocusToLeft => self.focus_neighbour(false),
            TabSelectionCommand::FocusToRight => self.focus_neighbour(true),
            TabSelectionCommand::FocusByIndex(index) => match self.selected.get(index) {
                Some(&at) => self.refocused(at),
                None => self.clone(),
            },
        }
    }

    /// The focused tab taken away: the focus goes to the next selected tab to its right, else to
    /// its left. `None` when it is the only one.
    pub fn without_focused(&self) -> Option<Self> {
        let focused = self.focused;
        let next = self
            .selected
            .iter()
            .copied()
            .find(|&p| p > focused)
            .or_else(|| self.selected.iter().copied().rev().find(|&p| p < focused))?;
        Some(self.keep(|p| p != focused, next))
    }

    /// Ctrl + click (Cmd on macOS) on the tab at `position`: an unselected tab joins the
    /// selection and takes the focus; a selected one leaves it (as `unselect_current` when it is
    /// the focused one), unless it is the only one.
    pub fn toggle(&self, position: usize) -> Self {
        if !self.contains(position) {
            return Self::new(self.selected.iter().copied(), position);
        }
        if position == self.focused {
            return self.without_focused().unwrap_or(self.clone());
        }
        self.keep(|p| p != position, self.focused)
    }

    /// Shift + click on the tab at `position`: the tabs from the focused one to it are the
    /// selection, and it takes the focus.
    pub fn extend_to(&self, position: usize) -> Self {
        let (from, to) = if position < self.focused {
            (position, self.focused)
        } else {
            (self.focused, position)
        };
        Self::new(from..=to, position)
    }

    /// `next_view` / `prev_view` with several tabs selected: the selection collapses to the
    /// selected tab next to the focused one that way (the focused one at the end), as in Sublime;
    /// the next press then leaves it.
    pub fn collapse_towards(&self, forward: bool) -> usize {
        let focused = self.focused;
        let next = if forward {
            self.selected.iter().copied().find(|&p| p > focused)
        } else {
            self.selected.iter().copied().rev().find(|&p| p < focused)
        };
        next.unwrap_or(focused)
    }

    fn focus_neighbour(&self, forward: bool) -> Self {
        self.refocused(self.collapse_towards(forward))
    }

    fn refocused(&self, focused: usize) -> Self {
        Self {
            selected: self.selected.clone(),
            focused,
        }
    }

    fn keep(&self, keep: impl Fn(usize) -> bool, focused: usize) -> Self {
        Self::new(self.selected.iter().copied().filter(|&p| keep(p)), focused)
    }
}
