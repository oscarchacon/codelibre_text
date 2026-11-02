use std::path::{Path, PathBuf};
use std::time::SystemTime;

use use_cases::gateways::{DirectoryEntry, FileSystemGateway};

/// [`FileSystemGateway`] on the real disk.
pub struct StdFileSystem;

impl FileSystemGateway for StdFileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, bytes)
    }

    fn modified(&self, path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        match std::fs::canonicalize(path) {
            Ok(absolute) => without_verbatim_prefix(absolute),
            Err(_) => path.to_path_buf(),
        }
    }

    fn list_dir(&self, path: &Path) -> Vec<DirectoryEntry> {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| DirectoryEntry {
                        is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                        path: e.path(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// `canonicalize` on Windows returns `\\?\C:\…`; keep the familiar `C:\…`.
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}
