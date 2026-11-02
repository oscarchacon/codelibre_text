use std::path::{Path, PathBuf};

use use_cases::gateways::ShellGateway;

/// [`ShellGateway`]: Explorer on Windows, Finder on macOS, `xdg-open` elsewhere.
pub struct OsShell;

impl OsShell {
    fn open_url_command(url: &str) -> std::process::Command {
        if cfg!(windows) {
            // Not `cmd /C start`: cmd would split the URL at every `&`.
            let mut command = std::process::Command::new("rundll32");
            command.args(["url.dll,FileProtocolHandler", url]);
            command
        } else {
            let mut command = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
            command.arg(url);
            command
        }
    }
}

impl ShellGateway for OsShell {
    fn open_in_file_manager(&self, path: &Path) {
        let _ = std::fs::create_dir_all(path);
        let program = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
            tracing::warn!("could not open {}: {e}", path.display());
        }
    }

    fn open_url(&self, url: &str) {
        if let Err(e) = Self::open_url_command(url).spawn() {
            tracing::warn!("could not open {url}: {e}");
        }
    }

    fn home_folder(&self) -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
    }
}
