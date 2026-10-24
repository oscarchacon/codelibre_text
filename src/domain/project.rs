//! Project management - Multi-root projects

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub folders: Vec<ProjectFolder>,
    pub build_systems: Vec<BuildSystem>,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFolder {
    pub path: String,
    pub folder_name: Option<String>,
    pub file_exclude: Vec<String>,
    pub folder_exclude: Vec<String>,
    pub follow_symlinks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSystem {
    pub name: String,
    pub cmd: Vec<String>,
    pub working_dir: Option<String>,
    pub env: Option<serde_json::Value>,
    pub shell: bool,
    pub file_regex: String,
    pub line_regex: String,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: Option<PathBuf>,
    pub settings: ProjectSettings,
    pub open_files: Vec<PathBuf>,
}

impl Project {
    pub fn new(name: String) -> Self {
        Self {
            name,
            path: None,
            settings: ProjectSettings {
                folders: Vec::new(),
                build_systems: Vec::new(),
                settings: serde_json::Value::Object(serde_json::Map::new()),
            },
            open_files: Vec::new(),
        }
    }

    pub fn add_folder(&mut self, path: PathBuf) {
        self.settings.folders.push(ProjectFolder {
            path: path.to_string_lossy().to_string(),
            folder_name: path.file_name().map(|n| n.to_string_lossy().to_string()),
            file_exclude: vec![".git".to_string(), "node_modules".to_string()],
            folder_exclude: vec![".git".to_string(), "node_modules".to_string(), ".vscode".to_string()],
            follow_symlinks: true,
        });
    }

    pub fn folders(&self) -> &[ProjectFolder] {
        &self.settings.folders
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }
}
