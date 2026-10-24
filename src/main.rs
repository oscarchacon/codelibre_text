//! CodeLibre Text - A lightweight, native text editor built with Rust + egui (no browser, no Electron)
//! Built with Rust + egui (no browser, no Electron)

mod domain;
mod services;
mod ui;
mod core;

use core::app::CodeLibreTextApp;
use core::cli::Cli;
use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1300.0, 900.0])
            .with_min_inner_size([600.0, 400.0])
            .with_resizable(true),
        ..Default::default()
    };

    if cli.fullscreen {
        let mut opts = options;
        opts.viewport = opts.viewport.with_fullscreen(true);
        eframe::run_native(
            "CodeLibre Text",
            opts,
            Box::new(|cc| {
                Ok(Box::new(CodeLibreTextApp::new(cc, cli.files)))
            }),
        )
    } else {
        eframe::run_native(
            "CodeLibre Text",
            options,
            Box::new(|cc| {
                Ok(Box::new(CodeLibreTextApp::new(cc, cli.files)))
            }),
        )
    }
    .map_err(|e| e.into())
}
