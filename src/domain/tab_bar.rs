//! Tab bar - Tab management for open files

use uuid::Uuid;

/// Tab state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    Normal,
    Modified,
    Unsaved,
    Dirty,
}

/// A tab in the tab bar
#[derive(Debug, Clone)]
pub struct Tab {
    pub buffer_id: Uuid,
    pub name: String,
    pub file_path: Option<String>,
    pub state: TabState,
    pub preview: bool,
    pub pinned: bool,
}

impl Tab {
    pub fn new(buffer_id: Uuid, name: String, file_path: Option<String>) -> Self {
        Self {
            buffer_id,
            name,
            file_path,
            state: TabState::Unsaved,
            preview: false,
            pinned: false,
        }
    }

    pub fn is_modified(&self) -> bool {
        self.state == TabState::Modified || self.state == TabState::Dirty
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.state = if modified { TabState::Modified } else { TabState::Normal };
    }

    pub fn display_name(&self) -> String {
        if self.is_modified() {
            format!("{} *", self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn as_preview(&mut self, preview: bool) {
        self.preview = preview;
    }

    pub fn is_preview(&self) -> bool {
        self.preview
    }

    pub fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }
}

/// Tab bar state
#[derive(Debug)]
pub struct TabBar {
    pub tabs: Vec<Tab>,
    pub active_tab: Option<usize>,
    pub context_tab: Option<usize>,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: None,
            context_tab: None,
        }
    }

    pub fn add_tab(&mut self, buffer_id: Uuid, name: String, file_path: Option<String>) -> usize {
        let tab = Tab::new(buffer_id, name, file_path);
        let index = self.tabs.len();
        self.tabs.push(tab);
        self.active_tab = Some(index);
        index
    }

    pub fn remove_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(index);
        if self.active_tab >= Some(index) && self.active_tab.is_some() {
            self.active_tab = Some(index.saturating_sub(1));
        }
        if self.active_tab.is_none() && !self.tabs.is_empty() {
            self.active_tab = Some(0);
        }
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = Some(index);
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab.and_then(|i| self.tabs.get(i))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_tab.and_then(|i| self.tabs.get_mut(i))
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active_tab
    }

    pub fn set_active_index(&mut self, index: Option<usize>) {
        self.active_tab = index.filter(|i| *i < self.tabs.len());
    }

    pub fn count(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn show_context_menu(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.context_tab = Some(index);
        }
    }

    pub fn hide_context_menu(&mut self) {
        self.context_tab = None;
    }

    pub fn has_context_menu(&self) -> bool {
        self.context_tab.is_some()
    }

    pub fn tab_at_index(&self, index: usize) -> Option<&Tab> {
        self.tabs.get(index)
    }
}
