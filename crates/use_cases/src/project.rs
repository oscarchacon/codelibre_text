//! Use cases: folders open in the side bar and the file index used by Goto Anything.

use std::path::{Path, PathBuf};

use crate::editor::Editor;
use crate::gateways::DirectoryEntry;

impl Editor {
    pub fn add_folder(&mut self, folder: PathBuf) {
        let folder = self.gateways.file_system.canonicalize(&folder);
        if !self.state.folders.contains(&folder) {
            self.state.folders.push(folder);
            self.refresh_file_index();
        }
    }

    /// Shows the native "Open Folder" dialog. Returns the folder that was added.
    pub fn prompt_add_folder(&mut self) -> Option<PathBuf> {
        let folder = self.gateways.dialogs.pick_folder()?;
        self.add_folder(folder.clone());
        Some(self.gateways.file_system.canonicalize(&folder))
    }

    pub fn remove_folder(&mut self, folder: &Path) {
        self.state.folders.retain(|f| f != folder);
        self.refresh_file_index();
    }

    pub fn close_all_folders(&mut self) {
        self.state.folders.clear();
        self.refresh_file_index();
    }

    /// Re-scans the open folders in the background.
    pub fn refresh_file_index(&mut self) {
        let settings = self.global_settings();
        self.gateways.file_index.rebuild(
            self.state.folders.clone(),
            settings.strings("folder_exclude_patterns"),
            settings.strings("file_exclude_patterns"),
        );
    }

    /// Collects a finished background scan; true if the file list changed.
    pub fn poll_file_index(&mut self) -> bool {
        self.gateways.file_index.poll()
    }

    pub fn is_indexing_files(&self) -> bool {
        self.gateways.file_index.is_indexing()
    }

    /// Entries of a folder for the side bar: folders first, sorted by name, without the
    /// `folder_exclude_patterns` / `file_exclude_patterns`.
    pub fn list_folder(&self, folder: &Path) -> Vec<DirectoryEntry> {
        let settings = self.global_settings();
        let folder_excludes = settings.strings("folder_exclude_patterns");
        let file_excludes = settings.strings("file_exclude_patterns");
        let mut entries: Vec<DirectoryEntry> = self
            .gateways
            .file_system
            .list_dir(folder)
            .into_iter()
            .filter(|e| {
                let name = e.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let patterns = if e.is_dir { &folder_excludes } else { &file_excludes };
                !patterns.iter().any(|pattern| matches_pattern(pattern, name))
            })
            .collect();
        entries.sort_by_key(|e| {
            let name = e.path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            (!e.is_dir, name)
        });
        entries
    }
}

/// Sublime-style exclude patterns: `*` and `?` wildcards matched against a file name.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    fn matches(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => matches(&p[1..], n) || (!n.is_empty() && matches(p, &n[1..])),
            (Some('?'), Some(_)) => matches(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => matches(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    matches(&p, &n)
}
