//! `codelibre_text` binary: parses the command line and opens the window.
//! Everything else lives in the library (`lib.rs`).

mod cli;

use std::path::PathBuf;

use clap::Parser;
use codelibre_text::app::App;
use codelibre_text::compose;

use cli::Cli;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CodeLibre Text")
        // Wayland matches the window to packaging/linux/codelibre_text.desktop (and its icon) by this id.
        .with_app_id("codelibre_text")
        .with_inner_size([1300.0, 860.0])
        .with_min_inner_size([480.0, 320.0]);
    // Window/taskbar icon on every platform; the .exe file icon comes from build.rs (Windows).
    match eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icons/codelibre_text.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => tracing::warn!("could not load the window icon: {e}"),
    }
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
