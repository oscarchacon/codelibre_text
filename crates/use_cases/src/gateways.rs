//! Gateway interfaces: everything the use cases need from the outside world.
//!
//! They are declared here, in the Use Cases circle, and implemented in the outer circles
//! (Interface Adapters for file formats, Frameworks & Drivers for libraries and the OS).
//! This keeps the dependency arrows pointing inwards.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use entities::document::DocumentId;
use entities::keymap::Keymap;
use entities::menu::{MenuItem, PaletteCommand};
use entities::settings::SettingsLayers;
use entities::symbol::Symbol;
use serde_json::{Map, Value};

use crate::session::Session;

/// Where packages live: our own `Packages` folder and, if installed, Sublime Text's.
#[derive(Debug, Clone)]
pub struct PackagePaths {
    pub packages: PathBuf,
    pub sublime_packages: Option<PathBuf>,
}

impl PackagePaths {
    pub fn user_dir(&self) -> PathBuf {
        self.packages.join("User")
    }

    pub fn user_settings_file(&self) -> PathBuf {
        self.user_dir().join("Preferences.sublime-settings")
    }

    /// Expands Sublime variables used in command arguments (`${packages}`, `${platform}`).
    pub fn expand(&self, path: &str) -> PathBuf {
        PathBuf::from(
            path.replace("${packages}", &self.packages.to_string_lossy())
                .replace("${platform}", platform_name()),
        )
    }
}

/// Sublime's platform name, used in file names such as `Default (Windows).sublime-keymap`.
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "OSX"
    } else {
        "Linux"
    }
}

pub struct DirectoryEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub trait FileSystemGateway {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
    fn modified(&self, path: &Path) -> Option<SystemTime>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// Absolute, normalized form of a path (or the path itself if that fails).
    fn canonicalize(&self, path: &Path) -> PathBuf;
    fn list_dir(&self, path: &Path) -> Vec<DirectoryEntry>;
}

/// Native file pickers.
pub trait DialogGateway {
    fn pick_files(&self, start_dir: Option<&Path>) -> Vec<PathBuf>;
    fn pick_folder(&self, start_dir: Option<&Path>) -> Option<PathBuf>;
    fn pick_save_path(&self, start_dir: Option<&Path>, file_name: &str) -> Option<PathBuf>;
}

pub trait ClipboardGateway {
    fn read_text(&self) -> Result<String, String>;
    fn write_text(&self, text: &str) -> Result<(), String>;
}

/// Opens folders in the operating system's file manager and links in the default browser.
pub trait ShellGateway {
    fn open_in_file_manager(&self, path: &Path);
    fn open_url(&self, url: &str);
}

/// Key bindings, menus and command palette entries read from packages.
pub struct UiResources {
    pub keymap: Keymap,
    pub main_menu: Vec<MenuItem>,
    pub palette: Vec<PaletteCommand>,
}

/// Sublime resources (`.sublime-settings`, `.sublime-keymap`, `.sublime-menu`, `.sublime-commands`).
pub trait ResourceGateway {
    fn paths(&self) -> &PackagePaths;
    /// All settings layers. Problems are appended to `errors`.
    fn load_settings(&self, errors: &mut Vec<String>) -> SettingsLayers;
    fn load_ui_resources(&self, include_sublime_packages: bool, errors: &mut Vec<String>) -> UiResources;
    /// The user's `Preferences.sublime-settings` as a JSON object.
    fn read_user_settings(&self) -> Map<String, Value>;
    fn write_user_settings(&self, values: &Map<String, Value>) -> Result<(), String>;
    /// Any other settings file (e.g. a plugin's `Foo.sublime-settings`), merged from every
    /// package in load order (`User` last). Problems are appended to `errors`.
    fn load_settings_file(&self, file_name: &str, include_sublime_packages: bool, errors: &mut Vec<String>) -> Map<String, Value>;
    /// The `User` package's copy of a settings file (what `save_settings` changes).
    fn read_user_settings_file(&self, file_name: &str) -> Map<String, Value>;
    fn write_user_settings_file(&self, file_name: &str, values: &Map<String, Value>) -> Result<(), String>;
    /// Contents of a built-in default resource (e.g. `Preferences.sublime-settings`).
    fn default_resource(&self, file_name: &str) -> Option<String>;
}

/// Syntax definitions and color schemes.
pub trait SyntaxGateway {
    /// Reloads syntaxes and color schemes from packages. Problems are appended to `errors`.
    fn reload(&mut self, include_sublime_packages: bool, errors: &mut Vec<String>);
    /// Syntax name for a file, from its name or its first line (`Plain Text` if unknown).
    fn detect(&self, path: &Path, first_line: &str) -> String;
    /// Resolves a syntax name or a `.sublime-syntax` path to a known syntax name.
    fn resolve(&self, name_or_path: &str) -> String;
    fn syntax_names(&self) -> Vec<String>;
    fn color_scheme_names(&self) -> Vec<String>;
    /// Functions, classes, headings… of a document, for Goto Symbol.
    fn symbols(&self, document: DocumentId, text: &str, syntax: &str) -> Vec<Symbol>;
    /// The document was closed; cached data about it can go.
    fn forget(&self, document: DocumentId);
}

/// Hot-exit storage.
pub trait SessionGateway {
    fn load(&self) -> Option<Session>;
    fn save(&self, session: &Session) -> Result<(), String>;
}

/// A package with Python plugins: its name and its folders (ours and Sublime's can both have one).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginPackage {
    pub name: String,
    pub dirs: Vec<PathBuf>,
}

/// What the plugin host needs to load the plugins.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginLoad {
    pub packages: Vec<PluginPackage>,
    /// `sublime.packages_path()`.
    pub packages_path: PathBuf,
    /// The `python_interpreter` setting; `None` finds one.
    pub interpreter: Option<String>,
}

/// Sublime's three kinds of commands (`TextCommand`, `WindowCommand`, `ApplicationCommand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCommandKind {
    Text,
    Window,
    Application,
}

/// A message from the plugin host.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginMessage {
    /// Plugin code calls the editor: a `sublime_api` function and its arguments.
    /// Answer it with [`PluginHostGateway::reply`].
    Call { function: String, args: Vec<Value> },
    /// Text that plugins printed.
    Log(String),
    /// The plugins are loaded: their commands, and a traceback for each plugin that failed.
    Loaded { commands: Vec<(String, PluginCommandKind)>, errors: Vec<String> },
    /// A command finished; `error` is its traceback if it failed.
    Done { error: Option<String> },
}

/// Runs Sublime Text plugins (Python) in a separate process. Every request is answered through
/// [`receive`](Self::receive): first any number of `Call`s and `Log`s, then `Loaded` or `Done`.
pub trait PluginHostGateway {
    /// (Re)starts the host and asks it to load the plugins.
    fn load(&mut self, request: &PluginLoad) -> Result<(), String>;
    /// Asks the host to run a plugin command on a view (text commands) or the window.
    fn run(&mut self, kind: PluginCommandKind, name: &str, args: &Value, view: Option<DocumentId>) -> Result<(), String>;
    /// The answer to the last `Call`.
    fn reply(&mut self, result: Result<Value, String>) -> Result<(), String>;
    /// Waits for the next message; an error means the host died or stopped answering.
    fn receive(&mut self) -> Result<PluginMessage, String>;
    fn stop(&mut self);
}

/// Background list of the files in the open folders (for Goto Anything).
pub trait FileIndexGateway {
    fn rebuild(&mut self, folders: Vec<PathBuf>, folder_excludes: Vec<String>, file_excludes: Vec<String>);
    /// Picks up a finished scan; true if the list changed.
    fn poll(&mut self) -> bool;
    fn files(&self) -> &[PathBuf];
    fn is_indexing(&self) -> bool;
}
