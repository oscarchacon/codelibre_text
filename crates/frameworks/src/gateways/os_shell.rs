use std::path::{Path, PathBuf};

use use_cases::gateways::ShellGateway;

/// [`ShellGateway`]: Explorer on Windows, Finder on macOS, `xdg-open` elsewhere (and the
/// freedesktop `FileManager1` D-Bus interface to show a file selected).
pub struct OsShell;

impl OsShell {
    fn open_url_command(url: &str) -> std::process::Command {
        if cfg!(windows) {
            // Not `cmd /C start`: cmd would split the URL at every `&`.
            let mut command = std::process::Command::new("rundll32");
            command.args(["url.dll,FileProtocolHandler", url]);
            command
        } else {
            let mut command = std::process::Command::new(if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            });
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

    fn select_in_file_manager(&self, file: &Path) {
        if !cfg!(windows) && !cfg!(target_os = "macos") {
            select_with_file_manager1(file);
            return;
        }
        let mut command = if cfg!(windows) {
            let mut command = std::process::Command::new("explorer");
            command.arg(format!("/select,{}", file.display()));
            command
        } else {
            let mut command = std::process::Command::new("open");
            command.arg("-R").arg(file);
            command
        };
        if let Err(e) = command.spawn() {
            tracing::warn!("could not show {}: {e}", file.display());
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

/// Linux and the BSDs: asks the file manager to show `file` selected through the freedesktop
/// `org.freedesktop.FileManager1.ShowItems` D-Bus call (Nautilus, Dolphin, Nemo, Caja, Thunar…
/// answer it), with `gdbus`. Without it, or when nobody answers, the file's folder opens instead.
/// Waits in a thread: the call may take a moment to start the file manager.
fn select_with_file_manager1(file: &Path) {
    let file = file.to_path_buf();
    std::thread::spawn(move || {
        let shown = std::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("['{}']", file_uri(&file)))
            .arg("")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !shown {
            let folder = file.parent().unwrap_or(&file);
            if let Err(e) = std::process::Command::new("xdg-open").arg(folder).spawn() {
                tracing::warn!("could not show {}: {e}", file.display());
            }
        }
    });
}

/// `file://` URI of an absolute path: every byte but the unreserved ones and `/` is
/// percent-encoded (spaces, `#`, `%`, quotes, non-ASCII names).
pub fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}
