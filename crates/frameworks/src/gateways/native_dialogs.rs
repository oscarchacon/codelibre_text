use std::path::{Path, PathBuf};

use use_cases::gateways::{DialogGateway, MessageAnswer, MessageButtons, MessageDialog, MessageLevel};

/// [`DialogGateway`] with the operating system's file pickers and message boxes (`rfd`).
pub struct NativeDialogs;

impl DialogGateway for NativeDialogs {
    fn pick_files(&self, start_dir: Option<&Path>) -> Vec<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_files().unwrap_or_default()
    }

    fn pick_folder(&self, start_dir: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    fn pick_save_path(&self, start_dir: Option<&Path>, file_name: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_file_name(file_name);
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        dialog.save_file()
    }

    fn show_message(&self, dialog: &MessageDialog) -> MessageAnswer {
        let level = match dialog.level {
            MessageLevel::Info => rfd::MessageLevel::Info,
            MessageLevel::Warning => rfd::MessageLevel::Warning,
            MessageLevel::Error => rfd::MessageLevel::Error,
        };
        let buttons = match &dialog.buttons {
            MessageButtons::Ok => rfd::MessageButtons::Ok,
            MessageButtons::OkCancel { ok, cancel } => rfd::MessageButtons::OkCancelCustom(ok.clone(), cancel.clone()),
            MessageButtons::YesNoCancel { yes, no, cancel } => {
                rfd::MessageButtons::YesNoCancelCustom(yes.clone(), no.clone(), cancel.clone())
            }
        };
        let result = rfd::MessageDialog::new()
            .set_level(level)
            .set_title(&dialog.title)
            .set_description(&dialog.text)
            .set_buttons(buttons)
            .show();
        // Custom buttons answer with their label.
        let (ok, no) = match &dialog.buttons {
            MessageButtons::Ok => ("", ""),
            MessageButtons::OkCancel { ok, .. } => (ok.as_str(), ""),
            MessageButtons::YesNoCancel { yes, no, .. } => (yes.as_str(), no.as_str()),
        };
        match result {
            rfd::MessageDialogResult::Ok | rfd::MessageDialogResult::Yes => MessageAnswer::Ok,
            rfd::MessageDialogResult::No => MessageAnswer::No,
            rfd::MessageDialogResult::Custom(label) if label == ok => MessageAnswer::Ok,
            rfd::MessageDialogResult::Custom(label) if label == no => MessageAnswer::No,
            _ => MessageAnswer::Cancel,
        }
    }
}
