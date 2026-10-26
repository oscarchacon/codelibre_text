//! # Frameworks & Drivers (outermost circle)
//!
//! CodeLibre Text: a native code editor compatible with Sublime Text resources.
//!
//! This crate holds everything tied to a specific library or to the operating system: the egui
//! views, syntect, native dialogs, the clipboard, the disk, and [`compose`], the composition
//! root that creates the gateway implementations and hands them to the use cases.
//!
//! It is a library plus a thin `main.rs` (the usual Rust layout for binaries), so the tests in
//! `tests/` can use it.

pub mod app;
pub mod gateways;
pub mod input;
pub mod syntax_engine;
pub mod views;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
use use_cases::{Editor, Gateways};

use gateways::{
    app_directories, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell,
    python_plugin_host::PythonPluginHost, std_file_system::StdFileSystem, syntect_syntaxes::SyntectSyntaxes,
    system_clipboard::SystemClipboard,
};
use syntax_engine::{SharedSyntaxEngine, SyntaxEngine};

/// Wires every gateway implementation into the use cases and starts the editor.
pub fn compose(paths: Vec<PathBuf>) -> (Editor, ViewState, SharedSyntaxEngine) {
    let package_paths = app_directories::package_paths();
    let session_file = app_directories::session_file(&package_paths);
    let engine: SharedSyntaxEngine = Rc::new(RefCell::new(SyntaxEngine::new(package_paths.clone())));
    let python_lib = app_directories::python_lib_dir(&package_paths);

    let gateways = Gateways {
        file_system: Box::new(StdFileSystem),
        dialogs: Box::new(NativeDialogs),
        clipboard: Box::new(SystemClipboard),
        resources: Box::new(SublimeResourceGateway::new(package_paths).with_system_language(sys_locale::get_locale())),
        syntaxes: Box::new(SyntectSyntaxes(engine.clone())),
        sessions: Box::new(SessionFileGateway::new(session_file)),
        file_index: Box::new(FolderIndex::default()),
        shell: Box::new(OsShell),
        plugins: Box::new(PythonPluginHost::new(python_lib)),
    };
    let mut editor = Editor::new(gateways);
    editor.start(paths);

    let mut view = ViewState::default();
    let settings = editor.global_settings();
    view.show_side_bar = settings.bool("show_side_bar", true);
    view.show_status_bar = settings.bool("show_status_bar", true);
    if settings.bool("hot_exit", true) {
        view.apply_layout(&editor.session_layout());
    }
    view.expanded_folders.extend(editor.state().folders.iter().cloned());
    (editor, view, engine)
}
