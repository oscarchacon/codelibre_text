//! # Test doubles for the gateways
//!
//! Not part of the application: in-memory implementations of every gateway trait declared in
//! `use_cases::gateways`, so use cases and interface adapters can be tested without a window,
//! a disk or a clipboard.
//!
//! A [`World`] holds the shared state of all the fakes. Prepare it (files, settings, key bindings…),
//! build an [`Editor`] from it, run use cases, then inspect what the editor did through the world
//! (files written, dialogs asked, folders opened…).
//!
//! ```ignore
//! let world = World::new().with_file("/p/a.txt", "hello");
//! let mut editor = world.editor();
//! editor.open_file(Path::new("/p/a.txt"));
//! assert_eq!(world.file_text("/p/a.txt"), "hello");
//! ```

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use entities::document::DocumentId;
use entities::keymap::{Binding, Keymap};
use entities::localization::{Catalog, Language};
use entities::menu::{ContextMenus, MenuItem, PaletteCommand};
use entities::settings::SettingsLayers;
use entities::symbol::Symbol;
use serde_json::{json, Map, Value};
use use_cases::gateways::{
    ArchiveFolder, ClipboardGateway, DialogGateway, DirectoryEntry, DownloadGateway, ErrorLogGateway, FileIndexGateway, FileSystemGateway, MessageAnswer, MessageDialog, PackagePaths,
    FileFilter, PluginCommandKind, PluginHostGateway, PluginLoad, PluginMessage, ProcessEvent, ProcessGateway, ProcessRequest,
    ResourceGateway, ScopeSpan, SessionGateway, ShellGateway, SyntaxGateway, UiResources, ContextFrame, SyntaxInfo,
};
use entities::meta_info::MetaPreference;
use use_cases::project::matches_pattern;
use use_cases::session::Session;
use use_cases::{Editor, Gateways};

/// Where the fake packages live.
pub const PACKAGES: &str = "/packages";
/// Where [`World::with_installed_packages`] puts our `Installed Packages`.
pub const INSTALLED_PACKAGES: &str = "/data/Installed Packages";

/// Syntaxes the fake syntax gateway knows, by file extension.
const SYNTAXES: &[(&str, &str)] = &[("rs", "Rust"), ("py", "Python"), ("json", "JSON"), ("md", "Markdown"), ("html", "HTML")];

// ----- file system ---------------------------------------------------------------------------------

#[derive(Default)]
pub struct MemoryDisk {
    files: RefCell<BTreeMap<PathBuf, (Vec<u8>, SystemTime)>>,
    dirs: RefCell<BTreeSet<PathBuf>>,
    clock: Cell<u64>,
    /// When set, every write fails with this message.
    pub fail_writes: RefCell<Option<String>>,
}

impl MemoryDisk {
    fn tick(&self) -> SystemTime {
        self.clock.set(self.clock.get() + 1);
        UNIX_EPOCH + Duration::from_secs(self.clock.get())
    }

    pub fn put(&self, path: impl AsRef<Path>, bytes: impl Into<Vec<u8>>) {
        let path = path.as_ref().to_path_buf();
        let time = self.tick();
        let mut parent = path.parent();
        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() {
                break;
            }
            self.dirs.borrow_mut().insert(dir.to_path_buf());
            parent = dir.parent();
        }
        self.files.borrow_mut().insert(path, (bytes.into(), time));
    }

    pub fn mkdir(&self, path: impl AsRef<Path>) {
        self.dirs.borrow_mut().insert(path.as_ref().to_path_buf());
    }

    pub fn bytes(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        self.files.borrow().get(path.as_ref()).map(|(b, _)| b.clone())
    }

    /// Deletes a folder and everything in it.
    pub fn remove_dir(&self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        self.dirs.borrow_mut().retain(|dir| !dir.starts_with(path));
        self.files.borrow_mut().retain(|file, _| !file.starts_with(path));
    }
}

struct FakeFileSystem(Rc<MemoryDisk>);

impl FileSystemGateway for FakeFileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.0.bytes(path).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(message) = self.0.fail_writes.borrow().clone() {
            return Err(std::io::Error::other(message));
        }
        self.0.put(path, bytes.to_vec());
        Ok(())
    }

    fn modified(&self, path: &Path) -> Option<SystemTime> {
        self.0.files.borrow().get(path).map(|(_, t)| *t)
    }

    fn exists(&self, path: &Path) -> bool {
        self.0.files.borrow().contains_key(path) || self.0.dirs.borrow().contains(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.0.dirs.borrow().contains(path)
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn list_dir(&self, path: &Path) -> Vec<DirectoryEntry> {
        let files = self.0.files.borrow();
        let dirs = self.0.dirs.borrow();
        let children = |p: &&PathBuf| p.parent() == Some(path);
        dirs.iter()
            .filter(children)
            .map(|p| DirectoryEntry { path: p.clone(), is_dir: true })
            .chain(files.keys().filter(children).map(|p| DirectoryEntry { path: p.clone(), is_dir: false }))
            .collect()
    }
}

// ----- dialogs, clipboard, shell -------------------------------------------------------------------

/// What the user "answers" in the native dialogs, and what they were asked.
#[derive(Default)]
pub struct DialogScript {
    pub files_to_open: RefCell<Vec<PathBuf>>,
    pub folder_to_open: RefCell<Option<PathBuf>>,
    /// What a dialog that picks several folders answers.
    pub folders_to_open: RefCell<Vec<PathBuf>>,
    /// The filters of the last file dialog.
    pub last_filters: RefCell<Vec<FileFilter>>,
    pub save_path: RefCell<Option<PathBuf>>,
    /// File names suggested in "Save As" dialogs.
    pub save_requests: RefCell<Vec<String>>,
    /// Start folder of the last "Open File" dialog.
    pub last_open_dir: RefCell<Option<PathBuf>>,
    /// Start folder of the last "Open Folder" dialog.
    pub last_folder_dir: RefCell<Option<PathBuf>>,
    /// Message dialogs shown, in order.
    pub messages: RefCell<Vec<MessageDialog>>,
    /// Answers to the next message dialogs (`Cancel` once they run out).
    pub message_answers: RefCell<VecDeque<MessageAnswer>>,
}

struct FakeDialogs(Rc<DialogScript>);

impl DialogGateway for FakeDialogs {
    fn pick_files(&self, start_dir: Option<&Path>, filters: &[FileFilter]) -> Vec<PathBuf> {
        *self.0.last_open_dir.borrow_mut() = start_dir.map(Path::to_path_buf);
        *self.0.last_filters.borrow_mut() = filters.to_vec();
        self.0.files_to_open.borrow().clone()
    }

    fn pick_folder(&self, start_dir: Option<&Path>) -> Option<PathBuf> {
        *self.0.last_folder_dir.borrow_mut() = start_dir.map(Path::to_path_buf);
        self.0.folder_to_open.borrow().clone()
    }

    fn pick_folders(&self, start_dir: Option<&Path>) -> Vec<PathBuf> {
        *self.0.last_folder_dir.borrow_mut() = start_dir.map(Path::to_path_buf);
        self.0.folders_to_open.borrow().clone()
    }

    fn pick_save_path(&self, _start_dir: Option<&Path>, file_name: &str, filters: &[FileFilter]) -> Option<PathBuf> {
        *self.0.last_filters.borrow_mut() = filters.to_vec();
        self.0.save_requests.borrow_mut().push(file_name.to_string());
        self.0.save_path.borrow().clone()
    }

    fn show_message(&self, dialog: &MessageDialog) -> MessageAnswer {
        self.0.messages.borrow_mut().push(dialog.clone());
        self.0.message_answers.borrow_mut().pop_front().unwrap_or(MessageAnswer::Cancel)
    }
}

struct FakeClipboard(Rc<RefCell<Result<String, String>>>);

impl ClipboardGateway for FakeClipboard {
    fn read_text(&self) -> Result<String, String> {
        self.0.borrow().clone()
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        *self.0.borrow_mut() = Ok(text.to_string());
        Ok(())
    }
}

struct FakeShell {
    folders: Rc<RefCell<Vec<PathBuf>>>,
    urls: Rc<RefCell<Vec<String>>>,
}

impl ShellGateway for FakeShell {
    fn open_in_file_manager(&self, path: &Path) {
        self.folders.borrow_mut().push(path.to_path_buf());
    }

    fn open_url(&self, url: &str) {
        self.urls.borrow_mut().push(url.to_string());
    }

    fn home_folder(&self) -> Option<PathBuf> {
        Some(PathBuf::from(HOME_FOLDER))
    }
}

/// The fake user's home folder.
pub const HOME_FOLDER: &str = "/home/user";
/// Where the fake error log is written, on the memory disk.
pub const ERROR_LOG: &str = "/data/Local/Errors.log";

/// Keeps what is logged (without times) and writes it to [`ERROR_LOG`] on the memory disk.
struct FakeErrorLog {
    disk: Rc<MemoryDisk>,
    errors: Rc<RefCell<Vec<String>>>,
}

impl ErrorLogGateway for FakeErrorLog {
    fn append(&self, errors: &[String]) {
        self.errors.borrow_mut().extend(errors.iter().cloned());
        self.disk.put(ERROR_LOG, self.errors.borrow().join("\n"));
    }

    fn path(&self) -> PathBuf {
        PathBuf::from(ERROR_LOG)
    }
}

// ----- Sublime resources ---------------------------------------------------------------------------

/// Settings, key bindings, menus and palette entries "found in packages".
pub struct Resources {
    pub paths: PackagePaths,
    /// `Preferences.sublime-settings` layers, in load order.
    pub preferences: RefCell<Vec<Value>>,
    /// `<Syntax>.sublime-settings` layers.
    pub syntax_settings: RefCell<Vec<(String, Value)>>,
    /// The user's `Preferences.sublime-settings` (loaded last, written by `persist_setting`).
    pub user_settings: RefCell<Map<String, Value>>,
    /// Other settings files as merged from the packages (`sublime.load_settings`).
    pub settings_files: RefCell<BTreeMap<String, Map<String, Value>>>,
    /// The `User` package's copies of other settings files (`sublime.save_settings`).
    pub user_settings_files: RefCell<BTreeMap<String, Map<String, Value>>>,
    pub bindings: RefCell<Vec<Binding>>,
    pub menu: RefCell<Vec<MenuItem>>,
    /// The right-click menus.
    pub context_menus: RefCell<ContextMenus>,
    pub palette: RefCell<Vec<PaletteCommand>>,
    /// Interface languages and their translations (`Localization/<language>.po`).
    pub translations: RefCell<Vec<(Language, Catalog)>>,
    /// The operating system's language.
    pub system_language: RefCell<Option<String>>,
    /// Errors reported while loading.
    pub errors: RefCell<Vec<String>>,
    pub settings_loads: Cell<usize>,
    /// `include_sublime_packages` of every `load_ui_resources` call.
    pub ui_loads: RefCell<Vec<bool>>,
    /// Package files by resource name (`Packages/<package>/<path>`).
    pub package_files: RefCell<BTreeMap<String, Vec<u8>>>,
    /// `include_sublime_packages` of every `unpack_packages` call.
    pub unpacks: RefCell<Vec<bool>>,
    /// `start_unpacking` unpacks in the background: until `unpacked` has its answer.
    pub background_unpacking: Cell<bool>,
    /// What `poll_unpacking` answers, once: whether the packages are to be loaded again.
    pub unpacked: Cell<Option<bool>>,
    /// The packages' `.tmPreferences`.
    pub meta_preferences: RefCell<Vec<MetaPreference>>,
}

struct FakeResources(Rc<Resources>);

impl ResourceGateway for FakeResources {
    fn paths(&self) -> &PackagePaths {
        &self.0.paths
    }

    fn load_settings(&self, errors: &mut Vec<String>) -> SettingsLayers {
        self.0.settings_loads.set(self.0.settings_loads.get() + 1);
        errors.extend(self.0.errors.borrow().iter().cloned());
        let mut layers = SettingsLayers::default();
        for layer in self.0.preferences.borrow().iter() {
            layers.add_preferences(layer);
        }
        layers.add_preferences(&Value::Object(self.0.user_settings.borrow().clone()));
        for (syntax, layer) in self.0.syntax_settings.borrow().iter() {
            layers.add_syntax_layer(syntax, layer.clone());
        }
        layers
    }

    fn load_ui_resources(&self, include_sublime: bool, _errors: &mut Vec<String>) -> UiResources {
        self.0.ui_loads.borrow_mut().push(include_sublime);
        let mut keymap = Keymap::default();
        for binding in self.0.bindings.borrow().iter() {
            keymap.add(binding.clone());
        }
        UiResources {
            keymap,
            main_menu: self.0.menu.borrow().clone(),
            context_menus: self.0.context_menus.borrow().clone(),
            palette: self.0.palette.borrow().clone(),
        }
    }

    fn read_user_settings(&self) -> Map<String, Value> {
        self.0.user_settings.borrow().clone()
    }

    fn write_user_settings(&self, values: &Map<String, Value>) -> Result<(), String> {
        *self.0.user_settings.borrow_mut() = values.clone();
        Ok(())
    }

    fn default_resource(&self, file_name: &str) -> Option<String> {
        (file_name == "Preferences.sublime-settings").then(|| "{ \"tab_size\": 4 }".to_string())
    }

    fn load_settings_file(&self, file_name: &str, _include_sublime: bool, _errors: &mut Vec<String>) -> Map<String, Value> {
        let mut values = self.0.settings_files.borrow().get(file_name).cloned().unwrap_or_default();
        values.extend(self.read_user_settings_file(file_name));
        values
    }

    fn read_user_settings_file(&self, file_name: &str) -> Map<String, Value> {
        self.0.user_settings_files.borrow().get(file_name).cloned().unwrap_or_default()
    }

    fn write_user_settings_file(&self, file_name: &str, values: &Map<String, Value>) -> Result<(), String> {
        self.0.user_settings_files.borrow_mut().insert(file_name.to_string(), values.clone());
        Ok(())
    }

    fn languages(&self, _include_sublime: bool) -> Vec<Language> {
        self.0.translations.borrow().iter().map(|(language, _)| language.clone()).collect()
    }

    fn load_translations(&self, language: &str, _include_sublime: bool, _errors: &mut Vec<String>) -> Catalog {
        let translations = self.0.translations.borrow();
        translations.iter().find(|(l, _)| l.code == language).map(|(_, c)| c.clone()).unwrap_or_default()
    }

    fn system_language(&self) -> Option<String> {
        self.0.system_language.borrow().clone()
    }

    fn find_resources(&self, pattern: &str, _include_sublime: bool) -> Vec<String> {
        let files = self.0.package_files.borrow();
        let file_name = |name: &str| name.rsplit('/').next().unwrap_or_default().to_string();
        files.keys().filter(|name| matches_pattern(pattern, &file_name(name))).cloned().collect()
    }

    fn load_resource(&self, name: &str, _include_sublime: bool) -> Result<Vec<u8>, String> {
        self.0.package_files.borrow().get(name).cloned().ok_or_else(|| format!("resource not found: {name}"))
    }

    fn unpack_packages(&self, include_sublime: bool, _errors: &mut Vec<String>) {
        self.0.unpacks.borrow_mut().push(include_sublime);
    }

    fn start_unpacking(&self, _include_sublime: bool) -> bool {
        self.0.background_unpacking.get()
    }

    fn poll_unpacking(&self) -> Option<bool> {
        self.0.unpacked.take()
    }

    fn load_json_resource(&self, name: &str, include_sublime: bool) -> Result<Value, String> {
        let bytes = self.load_resource(name, include_sublime)?;
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }

    fn load_meta_preferences(&self, _include_sublime: bool, _errors: &mut Vec<String>) -> Vec<MetaPreference> {
        self.0.meta_preferences.borrow().clone()
    }
}

// ----- syntaxes ------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Syntaxes {
    /// Symbols returned for any document (Goto Symbol).
    pub symbols: RefCell<Vec<Symbol>>,
    pub forgotten: RefCell<Vec<DocumentId>>,
    /// `include_sublime_packages` of every reload.
    pub reloads: RefCell<Vec<bool>>,
}

struct FakeSyntaxes(Rc<Syntaxes>);

impl SyntaxGateway for FakeSyntaxes {
    fn reload(&mut self, include_sublime: bool, _errors: &mut Vec<String>) {
        self.0.reloads.borrow_mut().push(include_sublime);
    }

    fn detect(&self, path: &Path, first_line: &str) -> String {
        let by_extension = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| SYNTAXES.iter().find(|(e, _)| *e == ext))
            .map(|(_, name)| name.to_string());
        by_extension.unwrap_or_else(|| {
            if first_line.starts_with("#!/usr/bin/env python") {
                "Python".into()
            } else {
                "Plain Text".into()
            }
        })
    }

    fn resolve(&self, name_or_path: &str) -> String {
        let stem = Path::new(name_or_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if self.syntax_names().contains(&stem) {
            stem
        } else {
            "Plain Text".into()
        }
    }

    fn syntax_path(&self, syntax: &str) -> String {
        match syntax {
            "Plain Text" => "Packages/Text/Plain text.tmLanguage".into(),
            name => format!("Packages/{name}/{name}.sublime-syntax"),
        }
    }

    fn base_scope(&self, syntax: &str) -> String {
        match syntax {
            "Plain Text" => "text.plain".into(),
            other => format!("source.{}", other.to_lowercase()),
        }
    }

    fn syntax_names(&self) -> Vec<String> {
        std::iter::once("Plain Text").chain(SYNTAXES.iter().map(|(_, n)| *n)).map(String::from).collect()
    }

    fn color_scheme_names(&self) -> Vec<String> {
        vec!["Daylight".into(), "Monokai".into(), "Solarized (dark)".into()]
    }

    /// White on black, and the scheme asked for under `scheme`.
    fn scheme_style(&self, scheme: &str) -> serde_json::Map<String, serde_json::Value> {
        let style = serde_json::json!({ "foreground": "#ffffff", "background": "#000000", "scheme": scheme });
        style.as_object().cloned().unwrap_or_default()
    }

    /// The globals above plus a red for `redish`. The scheme asked for under `scheme`.
    fn scheme_palette(&self, scheme: &str) -> serde_json::Map<String, serde_json::Value> {
        let palette = serde_json::json!({
            "background": "#000000", "foreground": "#ffffff", "redish": "#ff0000", "scheme": scheme,
        });
        palette.as_object().cloned().unwrap_or_default()
    }

    /// Strings are red and bold; the rest white. The scheme and scopes asked for under `scheme` / `scope`.
    fn scope_style(&self, scheme: &str, scope: &str) -> serde_json::Map<String, serde_json::Value> {
        let style = if scope.split_whitespace().any(|s| s.starts_with("string")) {
            serde_json::json!({ "foreground": "#ff0000", "bold": true, "scheme": scheme, "scope": scope })
        } else {
            serde_json::json!({ "foreground": "#ffffff", "scheme": scheme, "scope": scope })
        };
        style.as_object().cloned().unwrap_or_default()
    }

    fn symbols(&self, _document: DocumentId, _text: &str, _syntax: &str) -> Vec<Symbol> {
        self.0.symbols.borrow().clone()
    }

    /// The syntax's base scope; text between double quotes (quotes included) is also
    /// `string.quoted.double`.
    fn line_scopes(&self, _document: DocumentId, text: &str, syntax: &str, line: usize) -> Vec<ScopeSpan> {
        let Some(line_text) = text.split_inclusive('\n').nth(line) else { return Vec::new() };
        let base = self.base_scope(syntax);
        let string = format!("{base} string.quoted.double");
        let mut spans = Vec::new();
        let (mut start, mut quoted) = (0, false);
        for (at, c) in line_text.char_indices().filter(|(_, c)| *c == '"') {
            let end = if quoted { at + c.len_utf8() } else { at };
            if end > start {
                spans.push(ScopeSpan { range: start..end, scope: if quoted { string.clone() } else { base.clone() } });
            }
            (start, quoted) = (end, !quoted);
        }
        if start < line_text.len() {
            spans.push(ScopeSpan { range: start..line_text.len(), scope: if quoted { string } else { base } });
        }
        spans
    }

    fn document_scopes(&self, document: DocumentId, text: &str, syntax: &str) -> Vec<Vec<ScopeSpan>> {
        (0..text.split_inclusive('\n').count()).map(|line| self.line_scopes(document, text, syntax, line)).collect()
    }

    /// `main`, and `string` inside double quotes, of `Packages/<syntax>/<syntax>.sublime-syntax`.
    fn context_backtrace(&self, document: DocumentId, text: &str, syntax: &str, line: usize, column: usize) -> Vec<ContextFrame> {
        let file = format!("Packages/{syntax}/{syntax}.sublime-syntax");
        let frame = |name: &str| ContextFrame { context_name: name.to_string(), source_file: file.clone() };
        let in_string = self
            .line_scopes(document, text, syntax, line)
            .iter()
            .any(|s| s.range.start < column && column < s.range.end && s.scope.contains("string"));
        if in_string { vec![frame("main"), frame("string")] } else { vec![frame("main")] }
    }

    fn syntax_infos(&self) -> Vec<SyntaxInfo> {
        self.syntax_names()
            .into_iter()
            .map(|name| SyntaxInfo {
                path: format!("Packages/{name}/{name}.sublime-syntax"),
                scope: self.base_scope(&name),
                hidden: false,
                file_extensions: SYNTAXES.iter().filter(|(_, n)| *n == name).map(|(e, _)| e.to_string()).collect(),
                name,
            })
            .collect()
    }

    fn forget(&self, document: DocumentId) {
        self.0.forgotten.borrow_mut().push(document);
    }
}

// ----- plugin host ---------------------------------------------------------------------------------

/// One `run` request: (kind, command name, arguments, view).
pub type PluginRun = (PluginCommandKind, String, Value, Option<DocumentId>);

/// A scripted plugin host: a test queues the messages the "plugins" send (calls to the editor,
/// then `Loaded` / `Done`) and afterwards checks what the editor asked and answered.
#[derive(Default)]
pub struct PluginScript {
    /// What `receive` returns, in order.
    pub messages: RefCell<VecDeque<PluginMessage>>,
    /// What `try_receive` returns (messages sent outside a request, e.g. by a timer).
    pub idle_messages: RefCell<VecDeque<PluginMessage>>,
    pub loads: RefCell<Vec<PluginLoad>>,
    pub runs: RefCell<Vec<PluginRun>>,
    /// The `event` requests: (name, view).
    pub events: RefCell<Vec<(String, DocumentId)>>,
    /// The `callback` requests (a panel's answers): (callback id, name, arguments).
    pub callbacks: RefCell<Vec<(u64, String, Value)>>,
    /// The `query` requests (`on_query_context`, `on_text_command`…): (event, view, arguments).
    pub queries: RefCell<Vec<(String, Option<DocumentId>, Value)>>,
    /// The editor's answers to the `Call` messages, in order.
    pub replies: RefCell<Vec<Result<Value, String>>>,
    pub stops: Cell<usize>,
    /// Between the first `load` and a `stop`: a `load` then keeps the host.
    pub running: Cell<bool>,
    /// When set, `load` fails with this message (e.g. no Python installed).
    pub load_error: RefCell<Option<String>>,
}

impl PluginScript {
    /// Queues messages for the next request.
    pub fn then(&self, messages: impl IntoIterator<Item = PluginMessage>) {
        self.messages.borrow_mut().extend(messages);
    }

    /// Queues messages the plugins send while the editor is idle.
    pub fn meanwhile(&self, messages: impl IntoIterator<Item = PluginMessage>) {
        self.idle_messages.borrow_mut().extend(messages);
    }

    /// The names of the events sent so far.
    pub fn event_names(&self) -> Vec<String> {
        self.events.borrow().iter().map(|(name, _)| name.clone()).collect()
    }
}

/// `Call { function, args }`.
pub fn plugin_call(function: &str, args: Value) -> PluginMessage {
    PluginMessage::Call {
        function: function.to_string(),
        args: args.as_array().cloned().unwrap_or_default(),
    }
}

struct FakePluginHost(Rc<PluginScript>);

impl PluginHostGateway for FakePluginHost {
    fn load(&mut self, request: &PluginLoad) -> Result<bool, String> {
        self.0.loads.borrow_mut().push(request.clone());
        match self.0.load_error.borrow().clone() {
            Some(error) => Err(error),
            None => Ok(!self.0.running.replace(true)),
        }
    }

    fn run(&mut self, kind: PluginCommandKind, name: &str, args: &Value, view: Option<DocumentId>) -> Result<(), String> {
        self.0.runs.borrow_mut().push((kind, name.to_string(), args.clone(), view));
        Ok(())
    }

    fn event(&mut self, name: &str, view: DocumentId) -> Result<(), String> {
        self.0.events.borrow_mut().push((name.to_string(), view));
        Ok(())
    }

    fn callback(&mut self, id: u64, name: &str, args: &Value) -> Result<(), String> {
        self.0.callbacks.borrow_mut().push((id, name.to_string(), args.clone()));
        Ok(())
    }

    fn query(&mut self, name: &str, view: Option<DocumentId>, args: &Value) -> Result<(), String> {
        self.0.queries.borrow_mut().push((name.to_string(), view, args.clone()));
        Ok(())
    }

    fn reply(&mut self, result: Result<Value, String>) -> Result<(), String> {
        self.0.replies.borrow_mut().push(result);
        Ok(())
    }

    fn receive(&mut self) -> Result<PluginMessage, String> {
        self.0.messages.borrow_mut().pop_front().ok_or_else(|| "the plugin host stopped".to_string())
    }

    fn try_receive(&mut self) -> Result<Option<PluginMessage>, String> {
        Ok(self.0.idle_messages.borrow_mut().pop_front())
    }

    fn stop(&mut self) {
        self.0.stops.set(self.0.stops.get() + 1);
        self.0.running.set(false);
    }
}

// ----- processes -----------------------------------------------------------------------------------

/// A scripted process: a test queues what it writes and checks what was asked of it.
#[derive(Default)]
pub struct ProcessScript {
    /// Every process started.
    pub started: RefCell<Vec<ProcessRequest>>,
    /// What the next `poll` returns.
    pub events: RefCell<VecDeque<ProcessEvent>>,
    pub kills: Cell<usize>,
    /// When set, `start` fails with this message.
    pub start_error: RefCell<Option<String>>,
}

struct FakeProcesses(Rc<ProcessScript>);

impl ProcessGateway for FakeProcesses {
    fn start(&mut self, request: &ProcessRequest) -> Result<(), String> {
        self.0.started.borrow_mut().push(request.clone());
        match self.0.start_error.borrow().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn poll(&mut self) -> Vec<ProcessEvent> {
        self.0.events.borrow_mut().drain(..).collect()
    }

    fn kill(&mut self) {
        self.0.kills.set(self.0.kills.get() + 1);
    }
}

// ----- downloads -----------------------------------------------------------------------------------

/// Scripted downloads: the test sees what was asked and says when one finishes.
#[derive(Default)]
pub struct DownloadScript {
    /// Every download started: (url, destination).
    pub started: RefCell<Vec<(String, PathBuf)>>,
    /// What the next `poll` returns.
    pub finished: RefCell<Option<Result<(), String>>>,
}

struct FakeDownloads(Rc<DownloadScript>);

impl DownloadGateway for FakeDownloads {
    fn start(&mut self, url: &str, destination: &Path) -> Result<(), String> {
        self.0.started.borrow_mut().push((url.to_string(), destination.to_path_buf()));
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<(), String>> {
        self.0.finished.borrow_mut().take()
    }
}

// ----- session and file index ----------------------------------------------------------------------

struct FakeSessions(Rc<RefCell<Option<Session>>>);

impl SessionGateway for FakeSessions {
    fn load(&self) -> Option<Session> {
        self.0.borrow().clone()
    }

    fn save(&self, session: &Session) -> Result<(), String> {
        *self.0.borrow_mut() = Some(session.clone());
        Ok(())
    }
}

/// Arguments of one index rebuild: (folders, folder excludes, file excludes).
pub type Rebuild = (Vec<PathBuf>, Vec<String>, Vec<String>);

#[derive(Default)]
pub struct FileIndex {
    pub files: Vec<PathBuf>,
    /// Every rebuild requested by the editor.
    pub rebuilds: RefCell<Vec<Rebuild>>,
}

struct FakeFileIndex {
    shared: Rc<FileIndex>,
    files: Vec<PathBuf>,
    scanning: bool,
}

impl FileIndexGateway for FakeFileIndex {
    fn rebuild(&mut self, folders: Vec<PathBuf>, folder_excludes: Vec<String>, file_excludes: Vec<String>) {
        self.shared.rebuilds.borrow_mut().push((folders, folder_excludes, file_excludes));
        self.scanning = true;
    }

    fn poll(&mut self) -> bool {
        std::mem::take(&mut self.scanning)
    }

    fn files(&self) -> &[PathBuf] {
        &self.files
    }

    fn is_indexing(&self) -> bool {
        self.scanning
    }
}

// ----- the world -----------------------------------------------------------------------------------

/// Shared state of every fake gateway.
pub struct World {
    pub disk: Rc<MemoryDisk>,
    pub dialogs: Rc<DialogScript>,
    pub clipboard: Rc<RefCell<Result<String, String>>>,
    pub resources: Rc<Resources>,
    pub syntaxes: Rc<Syntaxes>,
    pub session: Rc<RefCell<Option<Session>>>,
    pub file_index: Rc<FileIndex>,
    /// Folders opened in the file manager.
    pub opened_folders: Rc<RefCell<Vec<PathBuf>>>,
    /// Links opened in the browser.
    pub opened_urls: Rc<RefCell<Vec<String>>>,
    pub plugins: Rc<PluginScript>,
    pub processes: Rc<ProcessScript>,
    pub downloads: Rc<DownloadScript>,
    /// Errors written to the error log.
    pub logged_errors: Rc<RefCell<Vec<String>>>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Default settings: 4-wide tabs, auto indent, hot exit, Unix line endings.
    pub fn new() -> Self {
        Self {
            disk: Rc::default(),
            dialogs: Rc::default(),
            clipboard: Rc::new(RefCell::new(Ok(String::new()))),
            resources: Rc::new(Resources {
                paths: PackagePaths {
                    packages: PathBuf::from(PACKAGES),
                    sublime_packages: None,
                    archive_folders: Vec::new(),
                },
                preferences: RefCell::new(vec![json!({
                    "tab_size": 4,
                    "translate_tabs_to_spaces": false,
                    "auto_indent": true,
                    "hot_exit": true,
                    "default_line_ending": "unix",
                    "color_scheme": "Monokai.sublime-color-scheme",
                    "font_size": 11,
                    "folder_exclude_patterns": [".git", "target"],
                    "file_exclude_patterns": ["*.pyc"],
                })]),
                syntax_settings: RefCell::default(),
                user_settings: RefCell::default(),
                settings_files: RefCell::default(),
                user_settings_files: RefCell::default(),
                bindings: RefCell::default(),
                menu: RefCell::default(),
                context_menus: RefCell::default(),
                palette: RefCell::default(),
                translations: RefCell::default(),
                system_language: RefCell::default(),
                errors: RefCell::default(),
                settings_loads: Cell::new(0),
                ui_loads: RefCell::default(),
                package_files: RefCell::default(),
                unpacks: RefCell::default(),
                background_unpacking: Cell::new(false),
                unpacked: Cell::new(None),
                meta_preferences: RefCell::default(),
            }),
            syntaxes: Rc::default(),
            session: Rc::default(),
            file_index: Rc::default(),
            opened_folders: Rc::default(),
            opened_urls: Rc::default(),
            plugins: Rc::default(),
            processes: Rc::default(),
            downloads: Rc::default(),
            logged_errors: Rc::default(),
        }
    }

    /// A package with a plugin file, so the editor starts the (fake) plugin host, which answers
    /// the load with these commands.
    pub fn with_plugin_commands(self, commands: &[(&str, PluginCommandKind)]) -> Self {
        self.with_plugins(commands, &[])
    }

    /// Like [`with_plugin_commands`](Self::with_plugin_commands), with listeners of these events.
    pub fn with_plugins(self, commands: &[(&str, PluginCommandKind)], events: &[&str]) -> Self {
        self.disk.put(format!("{PACKAGES}/MyPlugin/plugin.py"), "");
        self.plugins.then([PluginMessage::Loaded {
            commands: commands.iter().map(|(name, kind)| (name.to_string(), *kind)).collect(),
            events: events.iter().map(|e| e.to_string()).collect(),
            errors: Vec::new(),
        }]);
        self
    }

    pub fn with_file(self, path: &str, contents: impl Into<Vec<u8>>) -> Self {
        self.disk.put(path, contents);
        self
    }

    pub fn with_dir(self, path: &str) -> Self {
        self.disk.mkdir(path);
        self
    }

    /// A package file plugins can find and load (`Packages/<package>/<path>`).
    pub fn with_resource(self, name: &str, contents: impl Into<Vec<u8>>) -> Self {
        self.resources.package_files.borrow_mut().insert(name.to_string(), contents.into());
        self
    }

    /// A `.tmPreferences` of a package: where it applies and its `settings`.
    pub fn with_meta_preference(self, selector: &str, settings: Value) -> Self {
        let settings = settings.as_object().cloned().unwrap_or_default();
        let name = format!("Packages/Test/{}.tmPreferences", self.resources.meta_preferences.borrow().len());
        self.resources.meta_preferences.borrow_mut().push(MetaPreference { name, selector: selector.to_string(), settings });
        self
    }

    /// An extra `Preferences.sublime-settings` layer.
    pub fn with_preferences(self, layer: Value) -> Self {
        self.resources.preferences.borrow_mut().push(layer);
        self
    }

    pub fn with_syntax_settings(self, syntax: &str, layer: Value) -> Self {
        self.resources.syntax_settings.borrow_mut().push((syntax.to_string(), layer));
        self
    }

    pub fn with_binding(self, binding: Binding) -> Self {
        self.resources.bindings.borrow_mut().push(binding);
        self
    }

    pub fn with_palette(self, entry: PaletteCommand) -> Self {
        self.resources.palette.borrow_mut().push(entry);
        self
    }

    /// A language "found in packages", with `(English, translation)` pairs.
    pub fn with_language(self, code: &str, name: &str, messages: &[(&str, &str)]) -> Self {
        let mut catalog = Catalog::default();
        for (msgid, translation) in messages {
            catalog.insert(None, msgid, vec![translation.to_string()]);
        }
        let language = Language { code: code.into(), name: name.into() };
        self.resources.translations.borrow_mut().push((language, catalog));
        self
    }

    pub fn with_system_language(self, code: &str) -> Self {
        *self.resources.system_language.borrow_mut() = Some(code.into());
        self
    }

    pub fn with_menu(self, item: MenuItem) -> Self {
        self.resources.menu.borrow_mut().push(item);
        self
    }

    /// The right-click menus the packages "have".
    pub fn with_context_menus(self, menus: ContextMenus) -> Self {
        *self.resources.context_menus.borrow_mut() = menus;
        self
    }

    /// Files the background index "found" in the open folders.
    pub fn with_indexed_files(mut self, files: &[&str]) -> Self {
        Rc::get_mut(&mut self.file_index)
            .expect("set indexed files before building the editor")
            .files = files.iter().map(PathBuf::from).collect();
        self
    }

    /// Our own `Installed Packages` folder, [`INSTALLED_PACKAGES`] (empty).
    pub fn with_installed_packages(mut self) -> Self {
        Rc::get_mut(&mut self.resources)
            .expect("add Installed Packages before building the editor")
            .paths
            .archive_folders
            .push(ArchiveFolder {
                archives: PathBuf::from(INSTALLED_PACKAGES),
                unpacked: PathBuf::from("/data/Cache/own"),
                sublime: false,
                shipped: false,
            });
        self.disk.mkdir(INSTALLED_PACKAGES);
        self
    }

    pub fn with_session(self, session: Session) -> Self {
        *self.session.borrow_mut() = Some(session);
        self
    }

    fn gateways(&self) -> Gateways {
        Gateways {
            file_system: Box::new(FakeFileSystem(self.disk.clone())),
            dialogs: Box::new(FakeDialogs(self.dialogs.clone())),
            clipboard: Box::new(FakeClipboard(self.clipboard.clone())),
            resources: Box::new(FakeResources(self.resources.clone())),
            syntaxes: Box::new(FakeSyntaxes(self.syntaxes.clone())),
            sessions: Box::new(FakeSessions(self.session.clone())),
            file_index: Box::new(FakeFileIndex {
                shared: self.file_index.clone(),
                files: self.file_index.files.clone(),
                scanning: false,
            }),
            shell: Box::new(FakeShell {
                folders: self.opened_folders.clone(),
                urls: self.opened_urls.clone(),
            }),
            plugins: Box::new(FakePluginHost(self.plugins.clone())),
            processes: Box::new(FakeProcesses(self.processes.clone())),
            downloads: Box::new(FakeDownloads(self.downloads.clone())),
            error_log: Box::new(FakeErrorLog { disk: self.disk.clone(), errors: self.logged_errors.clone() }),
        }
    }

    /// An editor started with no paths (one empty untitled tab, or the restored session).
    pub fn editor(&self) -> Editor {
        self.editor_opening(&[])
    }

    /// An editor started like `codelibre_text <paths>`.
    pub fn editor_opening(&self, paths: &[&str]) -> Editor {
        let mut editor = Editor::new(self.gateways());
        editor.start(paths.iter().map(PathBuf::from).collect());
        editor
    }

    /// Text of a file on the fake disk.
    pub fn file_text(&self, path: &str) -> String {
        String::from_utf8(self.disk.bytes(path).unwrap_or_else(|| panic!("no file {path}"))).expect("utf-8")
    }

    pub fn file_exists(&self, path: &str) -> bool {
        self.disk.bytes(path).is_some()
    }
}

/// The text of the status bar message, if any.
pub fn status(editor: &Editor) -> String {
    editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default()
}

/// Opens a file that must exist and returns its text.
pub fn open(editor: &mut Editor, path: &str) -> String {
    assert!(editor.open_file(Path::new(path)), "could not open {path}: {}", status(editor));
    active_text(editor)
}

pub fn active_text(editor: &Editor) -> String {
    editor.active_document().map(|d| d.text.clone()).unwrap_or_default()
}
