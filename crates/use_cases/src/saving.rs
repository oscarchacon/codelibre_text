//! Use cases: save, save as, save all.

use entities::text_commands::{self, TrimOptions};

use crate::editor::Editor;

/// Saving one of these reloads the packages, so the change applies right away (as in Sublime).
const PACKAGE_RESOURCE_EXTENSIONS: &[&str] = &[
    ".sublime-keymap",
    ".sublime-menu",
    ".sublime-commands",
    ".sublime-syntax",
    ".sublime-color-scheme",
    ".tmTheme",
];

impl Editor {
    /// Saves the active document, asking for a path if it has none (or for `save_as`).
    /// Returns false if it was not saved.
    pub fn save_active(&mut self, save_as: bool) -> bool {
        let settings = self.view_settings();
        let start_dir = self.current_directory();
        let Some(doc) = self.active_document() else {
            return false;
        };
        if doc.read_only {
            self.notify("This view is read only");
            return false;
        }
        let path = match (&doc.path, save_as) {
            (Some(path), false) => path.clone(),
            _ => match self.gateways.dialogs.pick_save_path(start_dir.as_deref(), &doc.name()) {
                Some(path) => path,
                None => return false,
            },
        };

        let index = self.state.active;
        let doc = &mut self.state.documents[index];
        // `trim_trailing_white_space_on_save`: "none" / false, "all" / true, or "not_on_caret".
        let trim_on_caret = match settings.get("trim_trailing_white_space_on_save") {
            Some(v) if v.as_bool() == Some(true) || v.as_str() == Some("all") => Some(true),
            Some(v) if v.as_str() == Some("not_on_caret") => Some(false),
            _ => None,
        };
        if let Some(trim_on_caret) = trim_on_caret {
            let saved = doc.saved_text().to_string();
            let options = TrimOptions {
                only_modified_since: settings.bool("trim_only_modified_white_space", true).then_some(saved.as_str()),
                trim_on_caret,
            };
            doc.edit(|text, sel| Some(text_commands::trim_trailing_whitespace_with(text, sel, &options)));
        }
        if settings.bool("ensure_newline_at_eof_on_save", false) {
            text_commands::ensure_trailing_newline(&mut doc.text);
        }
        let was_untitled = doc.path.is_none();
        let bytes = doc.to_bytes();
        if let Err(e) = self.gateways.file_system.write(&path, &bytes) {
            self.notify(format!("{}: {e}", path.display()));
            return false;
        }
        let modified = self.gateways.file_system.modified(&path);
        let doc = &mut self.state.documents[index];
        doc.mark_saved(&path, modified);
        if was_untitled || save_as {
            doc.syntax = self.gateways.syntaxes.detect(&path, doc.first_line());
        }

        self.after_save(&path);
        self.notify(format!("Saved {}", path.display()));
        true
    }

    pub fn save_all(&mut self) {
        let active = self.state.active;
        for index in 0..self.state.documents.len() {
            let doc = &self.state.documents[index];
            if doc.is_dirty() && doc.path.is_some() {
                self.state.active = index;
                self.save_active(false);
            }
        }
        self.state.active = active;
    }

    /// Saving package resources reloads them; saving a plugin reloads the plugins, as in Sublime.
    fn after_save(&mut self, path: &std::path::Path) {
        if !path.starts_with(&self.gateways.resources.paths().packages) {
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.ends_with(".sublime-settings") {
            self.reload_settings();
        } else if PACKAGE_RESOURCE_EXTENSIONS.iter().any(|ext| name.ends_with(ext)) {
            self.load_packages();
        } else if name.ends_with(".py") {
            self.reload_plugins();
        }
    }
}
