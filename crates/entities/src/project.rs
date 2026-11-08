//! `.sublime-project` files: the folders a project opens, its build systems and the `$project*`
//! variables of its builds.
//!
//! The project is kept as the JSON object the file holds, so `set_project_data` can write back
//! whatever a plugin changed without losing the keys we do not read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::build::BuildSystem;

/// A window's project: its file (if it has one) and its data.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// The `.sublime-project` file; `None` for data set on a window that has no project file.
    pub file: Option<PathBuf>,
    /// The object the project file holds (`folders`, `build_systems`, `settings`, …).
    pub data: Map<String, Value>,
}

impl Project {
    /// A project from the object a `.sublime-project` file holds. `None` if it is not an object.
    pub fn from_json(file: Option<PathBuf>, data: &Value) -> Option<Self> {
        Some(Self { file, data: data.as_object()?.clone() })
    }

    /// The folders a project opens: the `folders` entries' `path`s, relative to the project file
    /// (or to `.` when there is no file). A missing `path` is the project's own folder.
    pub fn folder_paths(&self) -> Vec<PathBuf> {
        let base = self.file.as_deref().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
        self.data
            .get("folders")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|folder| folder.get("path").and_then(Value::as_str).or_else(|| folder.as_str()))
            .map(|path| if path == "." { base.clone() } else { base.join(path) })
            .collect()
    }

    /// Replaces the folders of the project (`add_folder` / `remove_folder` keep it up to date).
    pub fn set_folder_paths(&mut self, folders: &[PathBuf]) {
        let base = self.file.as_deref().and_then(Path::parent);
        let entries: Vec<Value> = folders
            .iter()
            .map(|folder| {
                let path = base.and_then(|base| folder.strip_prefix(base).ok()).unwrap_or(folder);
                json!({ "path": path.display().to_string() })
            })
            .collect();
        self.data.insert("folders".to_string(), Value::Array(entries));
    }

    /// The build systems a project defines (its `build_systems` array), by name.
    pub fn build_systems(&self, platform: &str) -> Vec<BuildSystem> {
        self.data
            .get("build_systems")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|build| {
                let name = build.get("name").and_then(Value::as_str)?;
                BuildSystem::from_project_json(&format!("project:{name}"), build, platform)
            })
            .collect()
    }

    /// `$project`, `$project_path`, `$project_name`, `$project_base_name` and `$project_extension`
    /// (empty for a project that has no file).
    pub fn variables(&self) -> HashMap<String, String> {
        let Some(file) = self.file.as_deref() else {
            return HashMap::from([
                ("project".to_string(), String::new()),
                ("project_path".to_string(), String::new()),
                ("project_name".to_string(), self.name()),
                ("project_base_name".to_string(), String::new()),
                ("project_extension".to_string(), String::new()),
            ]);
        };
        let part = |value: Option<&std::ffi::OsStr>| value.map(|v| v.to_string_lossy().into_owned()).unwrap_or_default();
        HashMap::from([
            ("project".to_string(), file.display().to_string()),
            ("project_path".to_string(), file.parent().map(|p| p.display().to_string()).unwrap_or_default()),
            ("project_name".to_string(), self.name()),
            ("project_base_name".to_string(), part(file.file_stem())),
            ("project_extension".to_string(), part(file.extension())),
        ])
    }

    /// The `name` of the project file, or the file's stem; empty without a file.
    fn name(&self) -> String {
        self.data
            .get("name")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| self.file.as_deref().and_then(Path::file_stem).map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_default()
    }

    /// `<project>.sublime-workspace` for a project file.
    pub fn workspace_file(&self) -> Option<PathBuf> {
        let file = self.file.as_deref()?;
        Some(file.with_extension("sublime-workspace"))
    }
}
