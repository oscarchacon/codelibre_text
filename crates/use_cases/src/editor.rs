//! The application state and the entry point of the use cases.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use entities::document::{Document, DocumentId, LineEnding};
use entities::find::FindOptions;
use entities::keymap::Keymap;
use entities::localization::Translations;
use entities::menu::{MenuItem, PaletteCommand};
use entities::settings::{Settings, SettingsLayers};

use crate::builds::{BuildOption, BuildState};
use crate::find_replace::FindCache;
use crate::gateways::{
    ClipboardGateway, DialogGateway, FileIndexGateway, FileSystemGateway, PluginHostGateway, ProcessGateway,
    ResourceGateway, SessionGateway, ShellGateway, SyntaxGateway,
};
use crate::output_panels::OutputPanels;
use crate::plugin_panels::PluginPanels;
use crate::plugins::PluginState;

/// The implementations of every gateway, provided by the outer circles at startup.
pub struct Gateways {
    pub file_system: Box<dyn FileSystemGateway>,
    pub dialogs: Box<dyn DialogGateway>,
    pub clipboard: Box<dyn ClipboardGateway>,
    pub resources: Box<dyn ResourceGateway>,
    pub syntaxes: Box<dyn SyntaxGateway>,
    pub sessions: Box<dyn SessionGateway>,
    pub file_index: Box<dyn FileIndexGateway>,
    pub shell: Box<dyn ShellGateway>,
    pub plugins: Box<dyn PluginHostGateway>,
    pub processes: Box<dyn ProcessGateway>,
}

/// A message for the status bar.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub shown_at: Instant,
}

/// Something waiting for the user to decide about unsaved changes.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingClose {
    /// Close these documents; the first one is being asked about.
    Documents(Vec<DocumentId>),
    /// Quit the application.
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseChoice {
    Save,
    DontSave,
    Cancel,
}

/// Everything the application knows. Read it through [`Editor::state`]; change it only through
/// the use case methods.
pub struct EditorState {
    pub documents: Vec<Document>,
    /// Index of the active document in `documents`.
    pub active: usize,
    pub folders: Vec<PathBuf>,
    pub settings: SettingsLayers,
    pub keymap: Keymap,
    pub main_menu: Vec<MenuItem>,
    pub palette: Vec<PaletteCommand>,
    /// Problems found while loading packages.
    pub load_errors: Vec<String>,
    /// The interface language (see [`crate::localization`]).
    pub translations: Rc<Translations>,
    pub find: FindOptions,
    pub replacement: String,
    pub find_error: Option<String>,
    pub status: Option<StatusMessage>,
    pub pending_close: Option<PendingClose>,
    /// What plugins printed, and their errors (View > Show Console).
    pub console: String,
    /// The last line put on the clipboard by copy/cut with nothing selected (pasted as a line).
    pub(crate) line_clipboard: Option<String>,
    pub(crate) plugins: PluginState,
    /// The quick / input panels plugins opened.
    pub(crate) plugin_panels: PluginPanels,
    /// The output panels plugins created (not tabs).
    pub(crate) output_panels: OutputPanels,
    pub(crate) builds: BuildState,
    /// What the build picker lists (read when it opens: `search` cannot load resources).
    pub(crate) build_options: Vec<BuildOption>,
    /// Most recently used first (for Ctrl+Tab).
    pub(crate) recently_used: Vec<DocumentId>,
    pub(crate) closed_files: Vec<PathBuf>,
    pub(crate) next_id: DocumentId,
    pub(crate) find_cache: Option<FindCache>,
    pub(crate) last_disk_check: Instant,
}

pub struct Editor {
    pub(crate) state: EditorState,
    pub(crate) gateways: Gateways,
}

impl Editor {
    pub fn new(gateways: Gateways) -> Self {
        Self {
            state: EditorState {
                documents: Vec::new(),
                active: 0,
                folders: Vec::new(),
                settings: SettingsLayers::default(),
                keymap: Keymap::default(),
                main_menu: Vec::new(),
                palette: Vec::new(),
                load_errors: Vec::new(),
                translations: Rc::default(),
                find: FindOptions::default(),
                replacement: String::new(),
                find_error: None,
                status: None,
                pending_close: None,
                console: String::new(),
                line_clipboard: None,
                plugins: PluginState::default(),
                plugin_panels: PluginPanels::default(),
                output_panels: OutputPanels::default(),
                builds: BuildState::default(),
                build_options: Vec::new(),
                recently_used: Vec::new(),
                closed_files: Vec::new(),
                next_id: 0,
                find_cache: None,
                last_disk_check: Instant::now(),
            },
            gateways,
        }
    }

    /// Startup: loads packages, restores the previous session and opens the given paths.
    pub fn start(&mut self, paths: Vec<PathBuf>) {
        self.load_packages();
        self.load_plugins();
        if self.global_settings().bool("hot_exit", true) {
            self.restore_session();
        }
        for path in paths {
            if self.gateways.file_system.is_dir(&path) {
                self.add_folder(path);
            } else {
                self.open_file(&path);
            }
        }
        if self.state.documents.is_empty() {
            self.new_file();
        }
        self.plugin_init();
        if !self.state.load_errors.is_empty() {
            self.notify(self.tr_n(
                "{count} package resource failed to load (Help > Package Load Errors)",
                "{count} package resources failed to load (Help > Package Load Errors)",
                self.state.load_errors.len(),
                &[],
            ));
        }
    }

    pub fn state(&self) -> &EditorState {
        &self.state
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.state.status = Some(StatusMessage {
            text: text.into(),
            shown_at: Instant::now(),
        });
    }

    pub fn clear_status(&mut self) {
        self.state.status = None;
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.state.documents.get(self.state.active)
    }

    pub(crate) fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.state.documents.get_mut(self.state.active)
    }

    pub(crate) fn index_of(&self, id: DocumentId) -> Option<usize> {
        self.state.documents.iter().position(|d| d.id == id)
    }

    pub(crate) fn next_document_id(&mut self) -> DocumentId {
        self.state.next_id += 1;
        self.state.next_id
    }

    pub(crate) fn default_line_ending(&self) -> LineEnding {
        let system = if cfg!(windows) { LineEnding::Windows } else { LineEnding::Unix };
        LineEnding::from_setting(self.global_settings().str("default_line_ending"), system)
    }

    /// Settings for the active document (including its syntax-specific settings).
    pub fn view_settings(&self) -> Settings {
        match self.active_document() {
            Some(doc) => self.settings_of(doc.id),
            None => self.global_settings(),
        }
    }

    /// Settings of one document (or output panel): its syntax-specific settings plus what plugins
    /// set on its view.
    pub(crate) fn settings_of(&self, id: DocumentId) -> Settings {
        let Some(doc) = self.any_document(id) else {
            return self.global_settings();
        };
        let settings = self.state.settings.for_syntax(&doc.syntax);
        match self.state.plugins.view_settings.get(&id) {
            Some(own) => settings.with(own),
            None => settings,
        }
    }

    pub fn global_settings(&self) -> Settings {
        self.state.settings.global()
    }

    /// The text of a document, for the editor widget to edit in place.
    /// (The text widget owns typing, caret movement and undo; see `docs/architecture.md`.)
    pub fn text_for_editing(&mut self, id: DocumentId) -> Option<&mut String> {
        let index = self.index_of(id)?;
        let doc = &mut self.state.documents[index];
        (!doc.read_only).then_some(&mut doc.text)
    }
}
