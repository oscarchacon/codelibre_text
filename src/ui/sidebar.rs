//! Sidebar - File tree panel (left sidebar)

use std::path::PathBuf;

/// File icon types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIcon {
    File,
    Folder,
    FolderOpen,
    GitFile,
    GitNew,
    GitModified,
    GitDeleted,
}

impl FileIcon {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::File => "\u{1F4C4}",
            Self::Folder => "\u{1F4C1}",
            Self::FolderOpen => "\u{1F4C2}",
            Self::GitFile => "\u{1F4C4}",
            Self::GitNew => "M \u{1F4C4}",
            Self::GitModified => "M \u{1F4C4}",
            Self::GitDeleted => "D \u{1F4C4}",
        }
    }
}

/// A node in the file tree
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub icon: FileIcon,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    pub depth: usize,
    pub git_status: Option<String>,
}

impl TreeNode {
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let is_dir = path.is_dir();
        let icon = if is_dir { FileIcon::Folder } else { FileIcon::File };

        Self {
            path,
            name,
            is_dir,
            icon,
            children: Vec::new(),
            expanded: false,
            depth: 0,
            git_status: None,
        }
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty() || self.is_dir
    }

    pub fn set_depth(&mut self, depth: usize) {
        self.depth = depth;
        for child in &mut self.children {
            child.set_depth(depth + 1);
        }
    }

    fn expand_all_node(&mut self) {
        self.expanded = true;
        for child in &mut self.children {
            child.expand_all_node();
        }
    }

    fn collapse_all_node(&mut self) {
        self.expanded = false;
        for child in &mut self.children {
            child.collapse_all_node();
        }
    }
}

/// Sidebar state
#[derive(Debug)]
pub struct Sidebar {
    pub tree: TreeNode,
    pub selected_path: Option<PathBuf>,
    pub context_menu_path: Option<PathBuf>,
    pub search_query: String,
    pub visible: bool,
    pub width: f32,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            tree: TreeNode::new(PathBuf::new()),
            selected_path: None,
            context_menu_path: None,
            search_query: String::new(),
            visible: true,
            width: 200.0,
        }
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width.max(100.0).min(500.0);
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn select_path(&mut self, path: PathBuf) {
        self.selected_path = Some(path);
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.selected_path.as_ref()
    }

    pub fn expand_all(&mut self) {
        self.tree.expand_all_node();
    }

    pub fn collapse_all(&mut self) {
        self.tree.collapse_all_node();
    }

    pub fn open_context_menu(&mut self, path: PathBuf) {
        self.context_menu_path = Some(path);
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu_path = None;
    }

    pub fn has_context_menu(&self) -> bool {
        self.context_menu_path.is_some()
    }
}
