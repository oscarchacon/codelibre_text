//! Use cases: File > Open Recent. The files opened from disk and the folders added are kept
//! newest first (`entities::recent`), for every window, and saved with the session.

use std::path::PathBuf;

use crate::editor::Editor;

impl Editor {
    /// The recently opened files, newest first (`window.file_history()`).
    pub fn file_history(&self) -> &[PathBuf] {
        self.state.recent_files.paths()
    }

    /// The recently added folders, newest first (`sublime.folder_history()`).
    pub fn folder_history(&self) -> &[PathBuf] {
        self.state.recent_folders.paths()
    }

    /// `open_recent_file {"index"}`: opens the file at `index` of the history.
    pub fn open_recent_file(&mut self, index: usize) {
        if let Some(path) = self.state.recent_files.get(index).map(PathBuf::from) {
            self.open_file(&path);
        }
    }

    /// `open_recent_folder {"index"}`: adds the folder at `index` of the history to this window
    /// when it is empty, else to a new window, as Sublime does.
    pub fn open_recent_folder(&mut self, index: usize) {
        let Some(folder) = self.state.recent_folders.get(index).map(PathBuf::from) else {
            return;
        };
        if !self.gateways.file_system.is_dir(&folder) {
            self.notify(self.tr_args(
                "{path} is not a folder any more",
                &[("path", &folder.display())],
            ));
            return;
        }
        if !self.state.folders.is_empty() || !self.state.documents.is_empty() {
            self.new_window();
        }
        self.add_folder(folder);
    }

    /// `clear_recent_files`: empties both lists, as Sublime's File > Open Recent > Clear Items.
    pub fn clear_recent_files(&mut self) {
        self.state.recent_files.clear();
        self.state.recent_folders.clear();
    }

    /// The lists a saved session had.
    pub(crate) fn restore_recent(&mut self, files: Vec<PathBuf>, folders: Vec<PathBuf>) {
        self.state.recent_files = entities::recent::RecentPaths::from_paths(files);
        self.state.recent_folders = entities::recent::RecentPaths::from_paths(folders);
    }
}
