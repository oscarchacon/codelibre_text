//! # Frameworks & Drivers (outermost circle)
//!
//! CodeLibre Text: a native code editor compatible with Sublime Text resources.
//!
//! This crate holds everything tied to a specific library or to the operating system: the egui
//! views, syntect, native dialogs, the clipboard, the disk, and `main`, which is the composition
//! root: it creates the gateway implementations and hands them to the use cases.

mod app;
mod cli;
mod gateways;
mod input;
mod syntax_engine;
mod views;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use clap::Parser;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
use use_cases::{Editor, Gateways};

use app::App;
use cli::Cli;
use gateways::{app_directories, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell,
               std_file_system::StdFileSystem, syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard};
use syntax_engine::{SharedSyntaxEngine, SyntaxEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CodeLibre Text")
        .with_inner_size([1300.0, 860.0])
        .with_min_inner_size([480.0, 320.0]);
    if cli.fullscreen {
        viewport = viewport.with_fullscreen(true);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "CodeLibre Text",
        options,
        Box::new(move |cc| {
            let paths: Vec<PathBuf> = cli.files.iter().map(PathBuf::from).collect();
            let (editor, view, engine) = compose(paths);
            Ok(Box::new(App::new(cc, editor, view, engine)))
        }),
    )
    .map_err(|e| e.into())
}

/// Wires every gateway implementation into the use cases and starts the editor.
fn compose(paths: Vec<PathBuf>) -> (Editor, ViewState, SharedSyntaxEngine) {
    let package_paths = app_directories::package_paths();
    let session_file = app_directories::session_file(&package_paths);
    let engine: SharedSyntaxEngine = Rc::new(RefCell::new(SyntaxEngine::new(package_paths.clone())));

    let gateways = Gateways {
        file_system: Box::new(StdFileSystem),
        dialogs: Box::new(NativeDialogs),
        clipboard: Box::new(SystemClipboard),
        resources: Box::new(SublimeResourceGateway::new(package_paths)),
        syntaxes: Box::new(SyntectSyntaxes(engine.clone())),
        sessions: Box::new(SessionFileGateway::new(session_file)),
        file_index: Box::new(FolderIndex::default()),
        shell: Box::new(OsShell),
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
