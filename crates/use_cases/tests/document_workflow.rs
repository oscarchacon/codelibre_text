//! Use cases exercised with in-memory gateways: no window, no disk.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use entities::document::DocumentId;
use entities::menu::{MenuItem, PaletteCommand};
use entities::settings::SettingsLayers;
use entities::symbol::Symbol;
use entities::find::FindOptions;
use entities::selection::Selection;
use serde_json::{json, Map, Value};
use use_cases::editing::TextCommand;
use use_cases::editor::{CloseChoice, PendingClose};
use use_cases::gateways::*;
use use_cases::session::Session;
use use_cases::{Editor, Gateways};

#[derive(Default)]
struct MemoryFiles(RefCell<HashMap<PathBuf, Vec<u8>>>);

struct FakeFileSystem(Rc<MemoryFiles>);

impl FileSystemGateway for FakeFileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.0 .0.borrow().get(path).cloned().ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.0 .0.borrow_mut().insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn modified(&self, _: &Path) -> Option<SystemTime> {
        None
    }
    fn exists(&self, path: &Path) -> bool {
        self.0 .0.borrow().contains_key(path)
    }
    fn is_dir(&self, _: &Path) -> bool {
        false
    }
    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }
    fn list_dir(&self, _: &Path) -> Vec<DirectoryEntry> {
        Vec::new()
    }
}

struct FakeDialogs;
impl DialogGateway for FakeDialogs {
    fn pick_files(&self, _: Option<&Path>) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_folder(&self) -> Option<PathBuf> {
        None
    }
    fn pick_save_path(&self, _: Option<&Path>, name: &str) -> Option<PathBuf> {
        Some(PathBuf::from(format!("/saved/{name}.txt")))
    }
}

struct FakeClipboard;
impl ClipboardGateway for FakeClipboard {
    fn read_text(&self) -> Result<String, String> {
        Ok("clip".into())
    }
}

struct FakeResources(PackagePaths);
impl ResourceGateway for FakeResources {
    fn paths(&self) -> &PackagePaths {
        &self.0
    }
    fn load_settings(&self, _: &mut Vec<String>) -> SettingsLayers {
        let mut layers = SettingsLayers::default();
        layers.add_preferences(&json!({ "tab_size": 4, "hot_exit": false, "default_line_ending": "unix" }));
        layers
    }
    fn load_ui_resources(&self, _: bool, _: &mut Vec<String>) -> UiResources {
        UiResources {
            keymap: Default::default(),
            main_menu: Vec::<MenuItem>::new(),
            palette: Vec::<PaletteCommand>::new(),
        }
    }
    fn read_user_settings(&self) -> Map<String, Value> {
        Map::new()
    }
    fn write_user_settings(&self, _: &Map<String, Value>) -> Result<(), String> {
        Ok(())
    }
    fn default_resource(&self, _: &str) -> Option<String> {
        None
    }
}

struct FakeSyntaxes;
impl SyntaxGateway for FakeSyntaxes {
    fn reload(&mut self, _: bool, _: &mut Vec<String>) {}
    fn detect(&self, path: &Path, _: &str) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("py") => "Python".into(),
            _ => "Plain Text".into(),
        }
    }
    fn resolve(&self, name: &str) -> String {
        name.into()
    }
    fn syntax_names(&self) -> Vec<String> {
        vec!["Plain Text".into(), "Python".into()]
    }
    fn color_scheme_names(&self) -> Vec<String> {
        vec!["Monokai".into()]
    }
    fn symbols(&self, _: DocumentId, _: &str, _: &str) -> Vec<Symbol> {
        Vec::new()
    }
    fn forget(&self, _: DocumentId) {}
}

struct NoSessions;
impl SessionGateway for NoSessions {
    fn load(&self) -> Option<Session> {
        None
    }
    fn save(&self, _: &Session) -> Result<(), String> {
        Ok(())
    }
}

struct NoFileIndex;
impl FileIndexGateway for NoFileIndex {
    fn rebuild(&mut self, _: Vec<PathBuf>, _: Vec<String>, _: Vec<String>) {}
    fn poll(&mut self) -> bool {
        false
    }
    fn files(&self) -> &[PathBuf] {
        &[]
    }
    fn is_indexing(&self) -> bool {
        false
    }
}

struct NoShell;
impl ShellGateway for NoShell {
    fn open_in_file_manager(&self, _: &Path) {}
}

fn editor_with_files(files: &[(&str, &[u8])]) -> (Editor, Rc<MemoryFiles>) {
    let memory = Rc::new(MemoryFiles::default());
    for (path, bytes) in files {
        memory.0.borrow_mut().insert(PathBuf::from(path), bytes.to_vec());
    }
    let gateways = Gateways {
        file_system: Box::new(FakeFileSystem(memory.clone())),
        dialogs: Box::new(FakeDialogs),
        clipboard: Box::new(FakeClipboard),
        resources: Box::new(FakeResources(PackagePaths {
            packages: PathBuf::from("/packages"),
            sublime_packages: None,
        })),
        syntaxes: Box::new(FakeSyntaxes),
        sessions: Box::new(NoSessions),
        file_index: Box::new(NoFileIndex),
        shell: Box::new(NoShell),
    };
    let mut editor = Editor::new(gateways);
    editor.start(Vec::new());
    (editor, memory)
}

#[test]
fn opening_a_file_replaces_the_empty_untitled_tab_and_detects_its_syntax() {
    let (mut editor, _) = editor_with_files(&[("/src/app.py", b"x = 1\n")]);
    assert!(editor.open_file(Path::new("/src/app.py")));
    assert_eq!(editor.state().documents.len(), 1);
    assert_eq!(editor.active_document().unwrap().syntax, "Python");
}

#[test]
fn edits_are_saved_with_the_original_line_endings() {
    let (mut editor, memory) = editor_with_files(&[("/a.txt", b"one\r\ntwo")]);
    editor.open_file(Path::new("/a.txt"));
    editor.run_text_command(TextCommand::DuplicateLine);
    assert!(editor.active_document().unwrap().is_dirty());

    assert!(editor.save_active(false));
    assert_eq!(memory.0.borrow()[Path::new("/a.txt")], b"one\r\none\r\ntwo".to_vec());
    assert!(!editor.active_document().unwrap().is_dirty());
}

#[test]
fn untitled_documents_ask_for_a_path_when_saved() {
    let (mut editor, memory) = editor_with_files(&[]);
    editor.run_text_command(TextCommand::Insert("hello".into()));
    assert!(editor.save_active(false));
    assert!(memory.0.borrow().contains_key(Path::new("/saved/hello.txt")));
}

#[test]
fn closing_a_dirty_document_asks_first() {
    let (mut editor, _) = editor_with_files(&[]);
    editor.run_text_command(TextCommand::Insert("draft".into()));
    editor.close_active_document();
    assert!(matches!(editor.state().pending_close, Some(PendingClose::Documents(_))));
    assert_eq!(editor.state().documents.len(), 1);

    editor.resolve_pending_close(CloseChoice::DontSave);
    assert!(editor.state().documents.is_empty());
}

#[test]
fn find_next_selects_the_next_match() {
    let (mut editor, _) = editor_with_files(&[("/f.txt", b"cat dog cat")]);
    editor.open_file(Path::new("/f.txt"));
    editor.set_find_options(FindOptions { query: "cat".into(), ..Default::default() });
    editor.find_next(true);
    editor.find_next(true);
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(8, 11));
    assert_eq!(editor.find_matches().len(), 2);
}

#[test]
fn reopens_the_last_closed_file() {
    let (mut editor, _) = editor_with_files(&[("/a.txt", b"a"), ("/b.txt", b"b")]);
    editor.open_file(Path::new("/a.txt"));
    editor.open_file(Path::new("/b.txt"));
    editor.close_active_document();
    assert_eq!(editor.state().documents.len(), 1);
    editor.reopen_closed_file();
    assert_eq!(editor.active_document().unwrap().name(), "b.txt");
}
