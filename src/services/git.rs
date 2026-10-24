//! Git integration - File status via git

use egui::Color32;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Untracked,
    Added,
    Modified,
    Deleted,
    Renamed,
    Conflicted,
    Ignored,
}

impl GitStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Untracked => "?",
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Conflicted => "!",
            Self::Ignored => "!",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Untracked => Color32::from_rgb(180, 180, 180),
            Self::Added => Color32::from_rgb(100, 220, 100),
            Self::Modified => Color32::from_rgb(255, 200, 50),
            Self::Deleted => Color32::from_rgb(255, 80, 80),
            Self::Renamed => Color32::from_rgb(100, 150, 255),
            Self::Conflicted => Color32::from_rgb(255, 120, 50),
            Self::Ignored => Color32::from_rgb(100, 100, 100),
        }
    }
}

#[derive(Debug)]
pub struct GitFileStatus {
    pub path: PathBuf,
    pub status: GitStatus,
    pub staged: bool,
}

/// Git status provider
#[derive(Debug, Default)]
pub struct GitProvider {
    pub root: Option<PathBuf>,
    pub file_statuses: Vec<GitFileStatus>,
}

impl GitProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
    }

    /// Scan git status for a file
    pub fn file_status(&self, file_path: &PathBuf) -> Option<GitStatus> {
        self.file_statuses.iter()
            .find(|fs| fs.path == *file_path)
            .map(|fs| fs.status)
    }

    /// Check if path is in a git repository
    pub fn is_in_git_repo(&self, path: &PathBuf) -> bool {
        self.root.as_ref().map(|r| path.starts_with(r)).unwrap_or(false)
    }

    /// Get git root for a path
    pub fn git_root(&self) -> Option<&PathBuf> {
        self.root.as_ref()
    }

    /// Refresh git status using git2
    pub fn refresh(&mut self) {
        if let Some(root) = &self.root {
            if let Ok(mut repo) = git2::Repository::open(root) {
                self.file_statuses.clear();

                let mut status_options = git2::StatusOptions::new();
                status_options.include_ignored(false);
                status_options.include_untracked(true);

                if let Ok(statuses) = repo.statuses(Some(&mut status_options)) {
                    for entry in statuses.iter() {
                        if let Some(path) = entry.path() {
                            let path_buf = root.join(path);
                            let status = match entry.status() {
                                git2::Status::CURRENT => GitStatus::Modified, // Working tree is modified
                                git2::Status::INDEX_NEW => GitStatus::Added,
                                git2::Status::WT_NEW => GitStatus::Untracked,
                                git2::Status::INDEX_DELETED => GitStatus::Deleted,
                                git2::Status::WT_DELETED => GitStatus::Deleted,
                                git2::Status::INDEX_MODIFIED => GitStatus::Modified,
                                git2::Status::WT_MODIFIED => GitStatus::Modified,
                                git2::Status::INDEX_RENAMED => GitStatus::Renamed,
                                git2::Status::WT_RENAMED => GitStatus::Renamed,
                                git2::Status::INDEX_TYPECHANGE => GitStatus::Modified,
                                git2::Status::WT_TYPECHANGE => GitStatus::Modified,
                                git2::Status::CONFLICTED => GitStatus::Conflicted,
                                _ => GitStatus::Modified,
                            };

                            self.file_statuses.push(GitFileStatus {
                                path: path_buf,
                                status,
                                staged: entry.status().is_index_new() || entry.status().is_index_modified() || entry.status().is_index_deleted(),
                            });
                        }
                    }
                }
            }
        }
    }
}
