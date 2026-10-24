use std::path::Path;

use use_cases::gateways::ShellGateway;

/// [`ShellGateway`]: Explorer on Windows, Finder on macOS, `xdg-open` elsewhere.
pub struct OsShell;

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
}
