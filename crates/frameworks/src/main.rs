//! `codelibre_text` binary: parses the command line and opens the window.
//! Everything else lives in the library (`lib.rs`).

mod cli;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use clap::Parser;
use codelibre_text::app::App;
use codelibre_text::compose;
use eframe::Renderer;

use cli::Cli;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let paths: Vec<PathBuf> = cli.files.iter().map(PathBuf::from).collect();

    // wgpu (Vulkan / DirectX 12 / Metal, then OpenGL) first. If it cannot start, e.g. on old GPUs,
    // virtual machines or remote desktops, open the window again with glow (plain OpenGL).
    // `WGPU_BACKEND=glow` goes straight to glow; other values (`vulkan`, `dx12`, `gl`…) are read by wgpu.
    let force_glow = std::env::var("WGPU_BACKEND").is_ok_and(|value| value.trim().eq_ignore_ascii_case("glow"));
    if force_glow {
        return run(Renderer::Glow, &cli, paths, &Rc::default());
    }
    let started = Rc::new(Cell::new(false));
    match run(Renderer::Wgpu, &cli, paths.clone(), &started) {
        // Only a failure before the editor started: an error after a session must not reopen it.
        Err(e) if !started.get() => {
            tracing::warn!("the wgpu renderer could not start ({e}); falling back to OpenGL (glow)");
            run(Renderer::Glow, &cli, paths, &started)
        }
        result => result,
    }
}

/// Opens the window with `renderer`; `started` is set once the editor is created.
fn run(renderer: Renderer, cli: &Cli, paths: Vec<PathBuf>, started: &Rc<Cell<bool>>) -> Result<(), Box<dyn std::error::Error>> {
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
    let options = eframe::NativeOptions { viewport, renderer, ..Default::default() };

    let started = started.clone();
    eframe::run_native(
        "CodeLibre Text",
        options,
        Box::new(move |cc| {
            started.set(true);
            let (editor, view, engine) = compose(paths);
            Ok(Box::new(App::new(cc, editor, view, engine)))
        }),
    )
    .map_err(|e| e.into())
}
