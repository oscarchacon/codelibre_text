//! Use cases: the side bar's commands for files and folders, as Sublime's `Default/side_bar.py`
//! has them: `new_file_at {"dirs"}`, `new_folder {"dirs"}`, `rename_path {"paths"}`,
//! `delete_file {"files", "prompt"}` and `delete_folder {"dirs"}`. Names are asked in the input
//! panel (`Folder Name:`, `New Name:`); deletes go to the system's trash after a confirmation.
//!
//! A plugin that defines one of these names (SideBarEnhancements, FileManager) runs instead: the
//! command controller only calls these when no plugin has the command.

use std::path::{Path, PathBuf};

use entities::command::Command;
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::gateways::{MessageAnswer, MessageButtons, MessageDialog, MessageLevel};
use crate::plugin_panels::BuiltinInput;
use crate::plugins::CommandRunner;

/// The side bar commands built in, which [`Editor::side_bar_command_visible`] knows about.
pub const SIDE_BAR_COMMANDS: &[&str] = &["new_file_at", "new_folder", "rename_path", "delete_file", "delete_folder"];

impl Editor {
    /// Whether a built-in side bar command shows in a menu with these arguments, as Sublime's
    /// `is_visible`: `new_file_at` / `new_folder` on one folder, `rename_path` on one path,
    /// `delete_file` on files, `delete_folder` on folders. Other commands always show.
    pub fn side_bar_command_visible(&self, command: &Command) -> bool {
        let count = |key: &str| paths(command, key).len();
        match command.name.as_str() {
            "new_file_at" | "new_folder" => count("dirs") == 1,
            "rename_path" => count("paths") == 1,
            "delete_file" => count("files") > 0,
            "delete_folder" => count("dirs") > 0,
            _ => true,
        }
    }

    /// `new_file_at {"dirs"}`: a new file whose first save starts in that folder (`default_dir`).
    pub fn new_file_at(&mut self, command: &Command) {
        self.new_file();
        let (Some(dir), Some(id)) = (paths(command, "dirs").into_iter().next(), self.active_document().map(|d| d.id)) else {
            return;
        };
        self.state.plugins.view_settings.entry(id).or_default().insert("default_dir".into(), json!(dir.display().to_string()));
    }

    /// `new_folder {"dirs"}`: asks for a name (`Folder Name:`), then makes it in the first folder.
    pub fn new_folder(&mut self, command: &Command, run_command: &mut CommandRunner) {
        let Some(dir) = paths(command, "dirs").into_iter().next() else { return };
        let caption = self.tr("Folder Name:").to_string();
        self.show_builtin_input_panel(caption, "", 0..0, BuiltinInput::NewFolder(dir), run_command);
    }

    /// `rename_path {"paths"}`: asks for a new name (`New Name:`) with the old one, its name
    /// without the extension selected for a file, then renames it.
    pub fn rename_path(&mut self, command: &Command, run_command: &mut CommandRunner) {
        let Some(path) = paths(command, "paths").into_iter().next() else { return };
        let Some(leaf) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
        let is_dir = self.gateways.file_system.is_dir(&path);
        let stem = match leaf.rfind('.') {
            Some(dot) if !is_dir && dot > 0 => leaf[..dot].chars().count(),
            _ => leaf.chars().count(),
        };
        let caption = self.tr("New Name:").to_string();
        self.show_builtin_input_panel(caption, &leaf, 0..stem, BuiltinInput::RenamePath(path), run_command);
    }

    /// `delete_file {"files", "prompt"}`: after asking (unless `prompt` is false), the files go to
    /// the trash. Their open views close, but those with unsaved changes stay open.
    pub fn delete_file(&mut self, command: &Command) {
        let files = paths(command, "files");
        if files.is_empty() {
            return;
        }
        if command.bool_arg("prompt", true) {
            let text = match files.as_slice() {
                [file] => self.tr_args("Delete File {path}?", &[("path", &file.display())]),
                _ => self.tr_n("Delete {count} File?", "Delete {count} Files?", files.len(), &[]),
            };
            if !self.confirm_delete(self.tr("Delete File").to_string(), text) {
                return;
            }
        }
        for file in &files {
            let clean: Vec<_> = self.state.documents.iter().filter(|d| d.path.as_ref() == Some(file) && !d.is_dirty()).map(|d| d.id).collect();
            self.close_documents(clean);
            self.trash(file);
        }
        self.state.folders_revision += 1;
    }

    /// `delete_folder {"dirs"}`: after asking, the folders go to the trash with what is in them.
    /// Files open from them stay open.
    pub fn delete_folder(&mut self, command: &Command) {
        let dirs = paths(command, "dirs");
        if dirs.is_empty() {
            return;
        }
        let text = match dirs.as_slice() {
            [dir] => self.tr_args("Delete Folder {path}?", &[("path", &dir.display())]),
            _ => self.tr_n("Delete {count} Folder?", "Delete {count} Folders?", dirs.len(), &[]),
        };
        if !self.confirm_delete(self.tr("Delete Folder").to_string(), text) {
            return;
        }
        for dir in &dirs {
            self.trash(dir);
        }
        self.state.folders_revision += 1;
    }

    /// The answer the user typed in a built-in command's input panel.
    pub(crate) fn finish_builtin_input(&mut self, input: BuiltinInput, text: &str) {
        let name = text.trim();
        if name.is_empty() {
            return;
        }
        match input {
            BuiltinInput::NewFolder(dir) => {
                if let Err(error) = self.gateways.file_system.create_dir(&dir.join(name)) {
                    self.notify(self.tr_args("Unable to create the folder: {error}", &[("error", &error)]));
                }
            }
            BuiltinInput::RenamePath(old) => self.rename(&old, name),
        }
        self.state.folders_revision += 1;
    }

    /// Renames `old` to `name` in the same folder. Open files follow it: those inside a renamed
    /// folder too, as Sublime's `view.retarget`.
    fn rename(&mut self, old: &Path, name: &str) {
        let new = old.with_file_name(name);
        if new == old {
            return;
        }
        let case_change = new.to_string_lossy().to_lowercase() == old.to_string_lossy().to_lowercase();
        if self.gateways.file_system.exists(&new) && !case_change {
            self.notify(self.tr_args("Unable to rename: {path} already exists", &[("path", &new.display())]));
            return;
        }
        if let Err(error) = self.gateways.file_system.rename(old, &new) {
            self.notify(self.tr_args("Unable to rename: {error}", &[("error", &error)]));
            return;
        }
        for index in 0..self.state.documents.len() {
            let Some(path) = self.state.documents[index].path.clone() else { continue };
            let Ok(inside) = path.strip_prefix(old) else { continue };
            let moved = if inside.as_os_str().is_empty() { new.clone() } else { new.join(inside) };
            let modified = self.gateways.file_system.modified(&moved);
            let extension_changed = moved.extension() != path.extension();
            let doc = &mut self.state.documents[index];
            if extension_changed {
                let first_line = doc.text.lines().next().unwrap_or_default().to_string();
                doc.syntax = self.gateways.syntaxes.detect(&moved, &first_line);
            }
            doc.path = Some(moved);
            doc.modified_at = modified;
        }
        if let Some(folder) = self.state.folders.iter_mut().find(|f| f.as_path() == old) {
            *folder = new;
            self.refresh_file_index();
        }
    }

    fn confirm_delete(&self, title: String, text: String) -> bool {
        let dialog = MessageDialog {
            level: MessageLevel::Warning,
            title,
            text,
            buttons: MessageButtons::OkCancel { ok: self.tr("Delete").to_string(), cancel: self.tr("Cancel").to_string() },
        };
        self.gateways.dialogs.show_message(&dialog) == MessageAnswer::Ok
    }

    fn trash(&mut self, path: &Path) {
        if let Err(error) = self.gateways.file_system.move_to_trash(path) {
            let message = self.tr_args("Unable to delete {path}: {error}", &[("path", &path.display()), ("error", &error)]);
            self.record_errors(std::slice::from_ref(&message));
            self.notify(message);
        }
    }
}

/// The paths of a list argument (`files`, `dirs`, `paths`).
fn paths(command: &Command, key: &str) -> Vec<PathBuf> {
    command.args.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(PathBuf::from).collect()
}
