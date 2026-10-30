//! Gateway interfaces: everything the use cases need from the outside world.
//!
//! They are declared here, in the Use Cases circle, and implemented in the outer circles
//! (Interface Adapters for file formats, Frameworks & Drivers for libraries and the OS).
//! This keeps the dependency arrows pointing inwards.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use entities::document::DocumentId;
use entities::keymap::Keymap;
use entities::localization::{Catalog, Language};
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
    /// Where `.sublime-package` files are, in load order.
    pub archive_folders: Vec<ArchiveFolder>,
}

/// A folder of `.sublime-package` files and where the editor keeps them unpacked: a folder of
/// packages like any other, with less priority than the unpacked ones.
#[derive(Debug, Clone)]
pub struct ArchiveFolder {
    pub archives: PathBuf,
    pub unpacked: PathBuf,
    /// It belongs to Sublime Text, so it follows `import_sublime_packages`.
    pub sublime: bool,
    /// The packages that come with Sublime Text. Their syntaxes use features the syntax engine
    /// lacks (it has its own for those languages), so they are left out.
    pub shipped: bool,
}

impl PackagePaths {
    /// The folders with one folder per package, in load order: what comes later wins.
    pub fn roots(&self, include_sublime: bool) -> Vec<PathBuf> {
        let unpacked = self.archive_folders.iter().filter(|f| include_sublime || !f.sublime).map(|f| f.unpacked.clone());
        let sublime = self.sublime_packages.iter().filter(|_| include_sublime).cloned();
        unpacked.chain(sublime).chain([self.packages.clone()]).collect()
    }

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

/// How serious a message dialog is (its icon).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

/// The buttons of a message dialog, with their labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageButtons {
    Ok,
    OkCancel { ok: String, cancel: String },
    YesNoCancel { yes: String, no: String, cancel: String },
}

/// The button the user pressed: `Ok` is also "Yes"; closing the dialog is `Cancel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAnswer {
    Ok,
    No,
    Cancel,
}

/// A modal message dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDialog {
    pub level: MessageLevel,
    pub title: String,
    pub text: String,
    pub buttons: MessageButtons,
}

/// A kind of file a dialog offers (e.g. `Text files`, with `txt` and `md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

/// Native file pickers and message dialogs.
pub trait DialogGateway {
    /// Any file when `filters` is empty.
    fn pick_files(&self, start_dir: Option<&Path>, filters: &[FileFilter]) -> Vec<PathBuf>;
    fn pick_folder(&self, start_dir: Option<&Path>) -> Option<PathBuf>;
    fn pick_folders(&self, start_dir: Option<&Path>) -> Vec<PathBuf>;
    fn pick_save_path(&self, start_dir: Option<&Path>, file_name: &str, filters: &[FileFilter]) -> Option<PathBuf>;
    /// Shows a message and waits for the user's answer.
    fn show_message(&self, dialog: &MessageDialog) -> MessageAnswer;
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
    /// Interface languages: the built-in ones plus those of `Localization/<language>.po` files in packages.
    fn languages(&self, include_sublime_packages: bool) -> Vec<Language>;
    /// Translations into `language`: the built-in ones, then every package's in load order (`User`
    /// last), later ones replacing earlier ones. Problems are appended to `errors`.
    fn load_translations(&self, language: &str, include_sublime_packages: bool, errors: &mut Vec<String>) -> Catalog;
    /// The operating system's language (e.g. `es-CL`), used when the `language` setting is `auto`.
    fn system_language(&self) -> Option<String>;
    /// Every package file whose name matches `pattern` (`*` and `?` wildcards), as resource names
    /// (`Packages/<package>/<path>`), in load order.
    fn find_resources(&self, pattern: &str, include_sublime_packages: bool) -> Vec<String>;
    /// The bytes of a resource named `Packages/<package>/<path>`.
    fn load_resource(&self, name: &str, include_sublime_packages: bool) -> Result<Vec<u8>, String>;
    /// Unpacks the `.sublime-package` files that changed. Problems are appended to `errors`.
    fn unpack_packages(&self, include_sublime_packages: bool, errors: &mut Vec<String>);
    /// A resource read as Sublime's relaxed JSON (e.g. a `.sublime-build`).
    fn load_json_resource(&self, name: &str, include_sublime_packages: bool) -> Result<Value, String>;
}

/// Syntax definitions and color schemes.
pub trait SyntaxGateway {
    /// Reloads syntaxes and color schemes from packages. Problems are appended to `errors`.
    fn reload(&mut self, include_sublime_packages: bool, errors: &mut Vec<String>);
    /// Syntax name for a file, from its name or its first line (`Plain Text` if unknown).
    fn detect(&self, path: &Path, first_line: &str) -> String;
    /// Resolves a syntax name or a `.sublime-syntax` path to a known syntax name.
    fn resolve(&self, name_or_path: &str) -> String;
    /// The scope of a whole file in a syntax (`source.python`); what build `selector`s match.
    fn base_scope(&self, syntax: &str) -> String;
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
    /// Where packages are unpacked and the folder of their archives: plugins there see the paths
    /// they would have inside the archive.
    pub archives: Vec<(PathBuf, PathBuf)>,
    /// The folder of the packages Sublime Text ships, if it is installed.
    pub shipped_packages: Option<PathBuf>,
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
    /// The plugins are loaded: their commands, the events they listen to (e.g. `on_modified`,
    /// `on_load_async`), and a traceback for each plugin that failed.
    Loaded { commands: Vec<(String, PluginCommandKind)>, events: Vec<String>, errors: Vec<String> },
    /// A command or event finished; `error` is its traceback if it failed.
    Done { error: Option<String> },
    /// The listeners' answer to a [`query`](PluginHostGateway::query) (`Null` when none answered).
    Answer(Value),
}

/// Runs Sublime Text plugins (Python) in a separate process. Every request is answered through
/// [`receive`](Self::receive): first any number of `Call`s and `Log`s, then `Loaded`, `Done` or `Answer`.
/// Between requests, timers and other threads of the plugins may call too
/// ([`try_receive`](Self::try_receive)).
pub trait PluginHostGateway {
    /// (Re)starts the host and asks it to load the plugins.
    fn load(&mut self, request: &PluginLoad) -> Result<(), String>;
    /// Asks the host to run a plugin command on a view (text commands) or the window.
    fn run(&mut self, kind: PluginCommandKind, name: &str, args: &Value, view: Option<DocumentId>) -> Result<(), String>;
    /// Tells the listeners about a view event (`on_load`, `on_modified`…).
    fn event(&mut self, name: &str, view: DocumentId) -> Result<(), String>;
    /// Answers a panel a plugin opened: callback `id`, which answer (`select`, `highlight`, `done`,
    /// `change`, `cancel`) and its arguments.
    fn callback(&mut self, id: u64, name: &str, args: &Value) -> Result<(), String>;
    /// Asks the listeners something they answer (`on_query_context`, `on_text_command`,
    /// `on_window_command`; also `on_post_*_command`, which answer nothing): the event, its view
    /// (none for window events) and the event's other arguments. Finished by an `Answer`.
    fn query(&mut self, name: &str, view: Option<DocumentId>, args: &Value) -> Result<(), String>;
    /// The answer to the last `Call` that has not been answered yet.
    fn reply(&mut self, result: Result<Value, String>) -> Result<(), String>;
    /// Waits for the next message; an error means the host died or stopped answering.
    fn receive(&mut self) -> Result<PluginMessage, String>;
    /// The next message if there is one already (calls made outside a request); never waits.
    fn try_receive(&mut self) -> Result<Option<PluginMessage>, String>;
    fn stop(&mut self);
}

/// What a build runs.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessCommand {
    /// A line for the system's shell.
    Shell(String),
    /// A program and its arguments.
    Program(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessRequest {
    pub command: ProcessCommand,
    pub working_dir: Option<PathBuf>,
    /// Variables to set. `$NAME` and `%NAME%` in their values are the process's own environment.
    pub env: Vec<(String, String)>,
    /// Replaces `PATH`, expanded like `env`.
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessEvent {
    /// Text the process wrote to its stdout or stderr.
    Output(String),
    /// It ended; `None` when it has no exit code (it was killed).
    Finished(Option<i32>),
}

/// Runs one external process at a time (a build), without waiting for it.
pub trait ProcessGateway {
    /// Starts a process; events of an earlier one are dropped.
    fn start(&mut self, request: &ProcessRequest) -> Result<(), String>;
    /// What happened since the last call; never waits.
    fn poll(&mut self) -> Vec<ProcessEvent>;
    /// Kills the process and its children. It still reports `Finished`.
    fn kill(&mut self);
}

/// Background list of the files in the open folders (for Goto Anything).
pub trait FileIndexGateway {
    fn rebuild(&mut self, folders: Vec<PathBuf>, folder_excludes: Vec<String>, file_excludes: Vec<String>);
    /// Picks up a finished scan; true if the list changed.
    fn poll(&mut self) -> bool;
    fn files(&self) -> &[PathBuf];
    fn is_indexing(&self) -> bool;
}
