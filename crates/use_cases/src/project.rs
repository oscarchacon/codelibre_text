//! Use cases: folders open in the side bar, `.sublime-project` files and the file index used by
//! Goto Anything.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use entities::project::Project;
use serde_json::Value;

use crate::editor::Editor;
use crate::gateways::{DirectoryEntry, FileFilter};

impl Editor {
    pub fn add_folder(&mut self, folder: PathBuf) {
        let folder = self.gateways.file_system.canonicalize(&folder);
        if !self.state.folders.contains(&folder) {
            self.state.folders.push(folder);
            self.refresh_file_index();
            self.sync_project_folders();
        }
    }

    /// Shows the native "Open Folder" dialog (in `initial_directory` if given). Returns the folder
    /// that was added.
    pub fn prompt_add_folder(&mut self, initial_directory: Option<&str>) -> Option<PathBuf> {
        let start = initial_directory.filter(|d| !d.is_empty()).map(|d| self.gateways.resources.paths().expand(d));
        let folder = self.gateways.dialogs.pick_folder(start.as_deref())?;
        self.add_folder(folder.clone());
        Some(self.gateways.file_system.canonicalize(&folder))
    }

    pub fn remove_folder(&mut self, folder: &Path) {
        self.state.folders.retain(|f| f != folder);
        self.refresh_file_index();
        self.sync_project_folders();
    }

    pub fn close_all_folders(&mut self) {
        self.state.folders.clear();
        self.refresh_file_index();
        self.sync_project_folders();
    }

    // ----- projects --------------------------------------------------------------------------

    /// Opens a `.sublime-project` file: its folders replace the window's and its build systems join
    /// Tools > Build System. False (and a message) when it cannot be read or is not an object.
    pub fn open_project(&mut self, path: &Path) -> bool {
        let path = self.gateways.file_system.canonicalize(path);
        let bytes = match self.gateways.file_system.read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.notify(format!("{}: {e}", path.display()));
                return false;
            }
        };
        let data: Value = match serde_json::from_slice(&bytes) {
            Ok(data) => data,
            Err(e) => {
                self.notify(self.tr_args("{name} is not a project: {error}", &[("name", &path.display()), ("error", &e.to_string())]));
                return false;
            }
        };
        let Some(project) = Project::from_json(Some(path.clone()), &data) else {
            self.notify(self.tr_args("{name} is not a project: it needs to be an object", &[("name", &path.display())]));
            return false;
        };
        self.set_folders(project.folder_paths());
        self.state.project = Some(project);
        self.refresh_menu_build_systems();
        true
    }

    /// Shows the native "Open Project" dialog.
    pub fn prompt_open_project(&mut self) {
        let filter = FileFilter { name: "Projects".to_string(), extensions: vec!["sublime-project".to_string()] };
        let start = self.state.folders.first().cloned();
        if let Some(path) = self.gateways.dialogs.pick_files(start.as_deref(), &[filter]).into_iter().next() {
            self.open_project(&path);
        }
    }

    /// Closes the project and its folders.
    pub fn close_project(&mut self) {
        if self.state.project.take().is_some() {
            self.set_folders(Vec::new());
            self.refresh_menu_build_systems();
        }
    }

    /// The window's `.sublime-project` file, if it has one.
    pub fn project_file(&self) -> Option<&Path> {
        self.state.project.as_ref().and_then(|p| p.file.as_deref())
    }

    /// `window.workspace_file_name()`: the project's `.sublime-workspace`.
    pub fn workspace_file(&self) -> Option<PathBuf> {
        self.state.project.as_ref().and_then(Project::workspace_file)
    }

    /// `window.project_data()`: the project's object, or a folder list for a window that only has
    /// folders open.
    pub fn project_data(&self) -> Value {
        match &self.state.project {
            Some(project) => Value::Object(project.data.clone()),
            None => Value::Object(
                [(
                    "folders".to_string(),
                    Value::Array(self.state.folders.iter().map(|folder| serde_json::json!({ "path": folder.display().to_string() })).collect()),
                )]
                .into_iter()
                .collect(),
            ),
        }
    }

    /// `window.set_project_data(data)`: the folders change to the data's, and the project file (if
    /// the window has one) is written. False when `data` is not an object.
    pub fn set_project_data(&mut self, data: Value) -> bool {
        let file = self.state.project.as_ref().and_then(|p| p.file.clone());
        let Some(mut project) = Project::from_json(file, &data) else {
            return false;
        };
        let folders = project.folder_paths();
        self.set_folders(folders);
        project.set_folder_paths(&self.state.folders);
        if let Some(path) = project.file.clone() {
            let text = serde_json::to_string_pretty(&Value::Object(project.data.clone())).unwrap_or_default();
            if let Err(e) = self.gateways.file_system.write(&path, text.as_bytes()) {
                self.notify(self.tr_args("Could not save {name}: {error}", &[("name", &path.display()), ("error", &e.to_string())]));
            }
        }
        self.state.project = Some(project);
        self.refresh_menu_build_systems();
        true
    }

    /// `$project*` variables of the window's project (empty strings when it has none).
    pub fn project_variables(&self) -> HashMap<String, String> {
        match &self.state.project {
            Some(project) => project.variables(),
            None => ["project", "project_path", "project_name", "project_base_name", "project_extension"]
                .into_iter()
                .map(|name| (name.to_string(), String::new()))
                .collect(),
        }
    }

    /// Replaces the folders at once (opening a project, restoring the session).
    fn set_folders(&mut self, folders: Vec<PathBuf>) {
        self.state.folders = folders.into_iter().map(|f| self.gateways.file_system.canonicalize(&f)).collect();
        self.refresh_file_index();
    }

    /// The side bar's folders and the project's `folders` stay the same.
    fn sync_project_folders(&mut self) {
        let folders = self.state.folders.clone();
        if let Some(project) = self.state.project.as_mut() {
            project.set_folder_paths(&folders);
        }
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
