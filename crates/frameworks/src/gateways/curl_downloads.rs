//! [`DownloadGateway`] that runs `curl`, which Windows 10 and later, macOS and most Linux systems
//! have, in a background thread that wakes the window when it ends.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use use_cases::gateways::DownloadGateway;

/// `CREATE_NO_WINDOW`: `curl` must not flash a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub struct CurlDownloads {
    waker: Option<Waker>,
    finished: Option<Receiver<Result<(), String>>>,
}

impl CurlDownloads {
    /// `waker` runs when a download ends, so the window handles it at once.
    pub fn with_waker(mut self, waker: Waker) -> Self {
        self.waker = Some(waker);
        self
    }
}

impl DownloadGateway for CurlDownloads {
    fn start(&mut self, url: &str, destination: &Path) -> Result<(), String> {
        if let Some(folder) = destination.parent() {
            std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        }
        // Next to the destination, so the rename at the end does not cross file systems.
        let partial = PathBuf::from(format!("{}.download", destination.display()));
        let mut command = Command::new("curl");
        command.args(["--fail", "--location", "--silent", "--show-error", "--output"]).arg(&partial).arg(url);
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        hide_window(&mut command);
        let child = command.spawn().map_err(|e| format!("curl: {e}"))?;

        let (sender, finished) = channel();
        let (destination, waker) = (destination.to_path_buf(), self.waker.clone());
        thread::spawn(move || {
            let result = match child.wait_with_output() {
                Ok(output) if output.status.success() => {
                    std::fs::rename(&partial, &destination).map_err(|e| format!("{}: {e}", destination.display()))
                }
                Ok(output) => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
                Err(e) => Err(format!("curl: {e}")),
            };
            if result.is_err() {
                let _ = std::fs::remove_file(&partial);
            }
            let _ = sender.send(result);
            if let Some(wake) = waker {
                wake();
            }
        });
        self.finished = Some(finished);
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<(), String>> {
        let result = self.finished.as_ref()?.try_recv().ok()?;
        self.finished = None;
        Some(result)
    }
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}
