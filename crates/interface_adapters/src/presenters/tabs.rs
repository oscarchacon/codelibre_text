use use_cases::Editor;

use crate::view_models::TabViewModel;

pub fn present(editor: &Editor) -> Vec<TabViewModel> {
    let state = editor.state();
    state
        .documents
        .iter()
        .enumerate()
        .map(|(index, doc)| TabViewModel {
            title: editor.document_name(doc),
            tooltip: doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| editor.document_name(doc)),
            dirty: doc.is_dirty(),
            active: index == state.active,
        })
        .collect()
}
