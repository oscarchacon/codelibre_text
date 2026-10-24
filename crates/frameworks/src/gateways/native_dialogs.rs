use std::path::{Path, PathBuf};

use use_cases::gateways::DialogGateway;

/// [`DialogGateway`] with the operating system's file pickers (`rfd`).
pub struct NativeDialogs;

impl DialogGateway for NativeDialogs {
    fn pick_files(&self, start_dir: Option<&Path>) -> Vec<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_files().unwrap_or_default()
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    fn pick_save_path(&self, start_dir: Option<&Path>, file_name: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_file_name(file_name);
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.save_file()
    }
}
