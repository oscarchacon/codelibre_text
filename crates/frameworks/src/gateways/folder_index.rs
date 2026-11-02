use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use use_cases::gateways::FileIndexGateway;
use use_cases::project::matches_pattern;

const MAX_FILES: usize = 100_000;

/// [`FileIndexGateway`] that walks folders on a background thread, respecting `.gitignore`
/// (`ignore` crate) and the exclude patterns.
#[derive(Default)]
pub struct FolderIndex {
    files: Vec<PathBuf>,
    scan: Option<Receiver<Vec<PathBuf>>>,
}

impl FileIndexGateway for FolderIndex {
    fn rebuild(&mut self, folders: Vec<PathBuf>, folder_excludes: Vec<String>, file_excludes: Vec<String>) {
        let (sender, receiver) = channel();
        self.scan = Some(receiver);
        std::thread::spawn(move || {
            let mut files = Vec::new();
            for folder in folders {
                let excluded_folders = folder_excludes.clone();
                let walker = ignore::WalkBuilder::new(&folder)
                    .hidden(false)
                    .git_ignore(true)
                    .filter_entry(move |entry| {
                        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                        let name = entry.file_name().to_string_lossy();
                        !(is_dir && excluded_folders.iter().any(|p| matches_pattern(p, &name)))
                    })
                    .build();
                for entry in walker.flatten() {
                    if !entry.file_type().is_some_and(|t| t.is_file()) {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy();
                    if file_excludes.iter().any(|p| matches_pattern(p, &name)) {
                        continue;
                    }
                    files.push(entry.into_path());
                    if files.len() >= MAX_FILES {
                        break;
                    }
                }
            }
            let _ = sender.send(files);
        });
    }

    fn poll(&mut self) -> bool {
        let Some(scan) = &self.scan else {
            return false;
        };
        match scan.try_recv() {
            Ok(files) => {
                self.files = files;
                self.scan = None;
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.scan = None;
                false
            }
            Err(TryRecvError::Empty) => false,
        }
    }

    fn files(&self) -> &[PathBuf] {
        &self.files
    }

    fn is_indexing(&self) -> bool {
        self.scan.is_some()
    }
}
