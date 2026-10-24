use use_cases::editor::PendingClose;
use use_cases::Editor;

use crate::view_models::{AboutViewModel, DialogViewModel};
use crate::view_state::ViewState;

/// The "save changes?" question, if one is pending.
pub fn present_close_dialog(editor: &Editor) -> Option<DialogViewModel> {
    let state = editor.state();
    match state.pending_close.as_ref()? {
        PendingClose::Documents(ids) => {
            let document_name = ids
                .first()
                .and_then(|id| state.documents.iter().find(|d| d.id == *id))
                .map(|d| d.name())
                .unwrap_or_default();
            Some(DialogViewModel::SaveChanges { document_name })
        }
        PendingClose::Exit => Some(DialogViewModel::ExitWithUnsavedChanges),
    }
}

pub fn present_about(editor: &Editor, view: &ViewState) -> Option<AboutViewModel> {
    if !view.show_about {
        return None;
    }
    let (packages, sublime_packages) = editor.packages_location();
    Some(AboutViewModel {
        version: env!("CARGO_PKG_VERSION"),
        packages: packages.display().to_string(),
        sublime_packages: sublime_packages.map(|p| p.display().to_string()),
    })
}

pub fn present_load_errors(editor: &Editor, view: &ViewState) -> Option<Vec<String>> {
    view.show_load_errors.then(|| editor.state().load_errors.clone())
}
