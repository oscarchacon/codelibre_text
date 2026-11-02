//! CLI argument parsing for CodeLibre Text

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "codelibre_text",
    version,
    about = "CodeLibre Text - Un editor de código nativo, ligero y de código libre",
    long_about = "CodeLibre Text - Un editor de código rápido y ligero construido con Rust y egui.
Soporta edición multi-cursor, resaltado de sintaxis, paleta de comandos, buscar y reemplazar,
barra lateral de archivos y sistema de plugins. Sin motor de navegador - puramente nativo."
)]
pub struct Cli {
    /// Files to open (multiple files open in tabs)
    #[arg(name = "files", num_args = 0..)]
    pub files: Vec<String>,

    /// Create a new untitled file
    #[arg(short, long, default_value_t = false)]
    pub new: bool,

    /// Open in fullscreen
    #[arg(short, long, default_value_t = false)]
    pub fullscreen: bool,

    /// Set font size
    #[arg(short = 's', long)]
    pub font_size: Option<f32>,

    /// Set theme (dark/light)
    #[arg(short, long, default_value = "dark")]
    pub theme: String,
}
