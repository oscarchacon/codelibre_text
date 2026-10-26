use std::path::{Path, PathBuf};

use use_cases::Editor;

use crate::view_models::{OpenFileViewModel, SideBarViewModel, TreeRowKind, TreeRowViewModel};
use crate::view_state::ViewState;

/// OPEN FILES plus the FOLDERS tree, flattened to the rows that are currently visible.
pub fn present(editor: &Editor, view: &mut ViewState) -> SideBarViewModel {
    let state = editor.state();
    let open_files = state
        .documents
        .iter()
        .enumerate()
        .map(|(index, doc)| OpenFileViewModel {
            name: editor.document_name(doc),
            dirty: doc.is_dirty(),
            active: index == state.active,
        })
        .collect();

    let active_path = editor.active_document().and_then(|d| d.path.clone());
    let mut folders = Vec::new();
    for root in &state.folders {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let expanded = view.expanded_folders.contains(root);
        folders.push(TreeRowViewModel {
            depth: 0,
            name,
            path: root.clone(),
            kind: TreeRowKind::Folder { expanded, is_root: true },
            active: false,
        });
        if expanded {
            add_children(editor, view, root, 1, active_path.as_deref(), &mut folders);
        }
    }
    SideBarViewModel { open_files, folders }
}

fn add_children(
    editor: &Editor,
    view: &mut ViewState,
    folder: &Path,
    depth: usize,
    active_path: Option<&Path>,
    rows: &mut Vec<TreeRowViewModel>,
) {
    let entries = listing(editor, view, folder);
    for (path, is_dir) in entries {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if is_dir {
            let expanded = view.expanded_folders.contains(&path);
            rows.push(TreeRowViewModel {
                depth,
                name,
                path: path.clone(),
                kind: TreeRowKind::Folder { expanded, is_root: false },
                active: false,
            });
            if expanded {
                add_children(editor, view, &path, depth + 1, active_path, rows);
            }
        } else {
            rows.push(TreeRowViewModel {
                depth,
                name,
                active: active_path == Some(path.as_path()),
                path,
                kind: TreeRowKind::File,
            });
        }
    }
}

/// Folder contents are read once and cached until "Refresh Folders".
fn listing(editor: &Editor, view: &mut ViewState, folder: &Path) -> Vec<(PathBuf, bool)> {
    view.folder_listings
        .entry(folder.to_path_buf())
        .or_insert_with(|| {
            editor
                .list_folder(folder)
                .into_iter()
                .map(|entry| (entry.path, entry.is_dir))
                .collect()
        })
        .clone()
}
