//! [`SessionGateway`] that stores the session as JSON in `Local/Session.sublime_session`.

use std::path::PathBuf;

use entities::document::RegionSet;
use entities::layout::Layout;
use entities::selection::Selection;
use serde::{Deserialize, Serialize};
use use_cases::gateways::SessionGateway;
use use_cases::session::{Session, SessionDocument};

pub struct SessionFileGateway {
    path: PathBuf,
}

impl SessionFileGateway {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SessionGateway for SessionFileGateway {
    fn load(&self) -> Option<Session> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str::<SessionFile>(&text).ok().map(Into::into)
    }

    fn save(&self, session: &Session) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&SessionFile::from(session)).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }
}

/// The file format. Kept separate from `Session` so the use cases do not depend on serde.
#[derive(Serialize, Deserialize)]
struct SessionFile {
    folders: Vec<PathBuf>,
    /// The window's `.sublime-project`, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project: Option<PathBuf>,
    #[serde(default)]
    documents: Vec<SessionFileDocument>,
    #[serde(default)]
    active: usize,
    #[serde(default)]
    layout: Vec<(String, bool)>,
    /// Sublime's `set_layout` value, when the window has several groups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    groups: Option<serde_json::Value>,
    #[serde(default)]
    active_group: usize,
    /// The other windows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    windows: Vec<SessionFile>,
}

#[derive(Serialize, Deserialize)]
struct SessionFileDocument {
    path: Option<PathBuf>,
    unsaved_text: Option<String>,
    syntax: String,
    selection: (usize, usize),
    #[serde(default)]
    group: usize,
    /// The `PERSISTENT` region sets of the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    regions: Vec<SessionFileRegion>,
}

#[derive(Serialize, Deserialize)]
struct SessionFileRegion {
    key: String,
    regions: Vec<(usize, usize)>,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    flags: u64,
    #[serde(default)]
    annotations: Vec<String>,
    #[serde(default)]
    annotation_color: String,
}

impl From<&Session> for SessionFile {
    fn from(session: &Session) -> Self {
        Self {
            folders: session.folders.clone(),
            project: session.project.clone(),
            documents: session
                .documents
                .iter()
                .map(|d| SessionFileDocument {
                    path: d.path.clone(),
                    unsaved_text: d.unsaved_text.clone(),
                    syntax: d.syntax.clone(),
                    selection: (d.selection.anchor, d.selection.head),
                    group: d.group,
                    regions: d.regions.iter().map(region_to_file).collect(),
                })
                .collect(),
            active: session.active,
            layout: session.layout.clone(),
            groups: session.groups.as_ref().map(Layout::to_json),
            active_group: session.active_group,
            windows: session.windows.iter().map(SessionFile::from).collect(),
        }
    }
}

impl From<SessionFile> for Session {
    fn from(file: SessionFile) -> Self {
        Self {
            folders: file.folders,
            project: file.project,
            documents: file
                .documents
                .into_iter()
                .map(|d| SessionDocument {
                    path: d.path,
                    unsaved_text: d.unsaved_text,
                    syntax: d.syntax,
                    selection: Selection::new(d.selection.0, d.selection.1),
                    group: d.group,
                    regions: d.regions.into_iter().map(region_from_file).collect(),
                })
                .collect(),
            active: file.active,
            layout: file.layout,
            groups: file.groups.as_ref().and_then(Layout::from_json),
            active_group: file.active_group,
            windows: file.windows.into_iter().map(Session::from).collect(),
        }
    }
}

fn region_to_file(set: &RegionSet) -> SessionFileRegion {
    SessionFileRegion {
        key: set.key.clone(),
        regions: set.regions.clone(),
        scope: set.scope.clone(),
        icon: set.icon.clone(),
        flags: set.flags,
        annotations: set.annotations.clone(),
        annotation_color: set.annotation_color.clone(),
    }
}

fn region_from_file(set: SessionFileRegion) -> RegionSet {
    RegionSet {
        key: set.key,
        regions: set.regions,
        scope: set.scope,
        icon: set.icon,
        flags: set.flags,
        annotations: set.annotations,
        annotation_color: set.annotation_color,
    }
}
