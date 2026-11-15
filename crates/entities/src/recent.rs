//! Entities: the files and folders opened last (File > Open Recent, `window.file_history()`,
//! `sublime.folder_history()`), newest first, each once.

use std::path::{Path, PathBuf};

/// How many paths a list keeps (Sublime's menu shows the first 8 of each).
pub const RECENT_LIMIT: usize = 30;

/// Recently opened paths, newest first, without duplicates, at most [`RECENT_LIMIT`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentPaths {
    paths: Vec<PathBuf>,
}

impl RecentPaths {
    /// A list from saved paths (newest first); duplicates after the first and paths past the
    /// limit are dropped.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        let mut recent = Self::default();
        for path in paths.into_iter().rev() {
            recent.push(path);
        }
        recent
    }

    /// `path` was opened: it goes first (moved there if it was already listed).
    pub fn push(&mut self, path: PathBuf) {
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(RECENT_LIMIT);
    }

    pub fn get(&self, index: usize) -> Option<&Path> {
        self.paths.get(index).map(PathBuf::as_path)
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }
}
