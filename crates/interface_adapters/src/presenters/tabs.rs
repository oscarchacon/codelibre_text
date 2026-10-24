use use_cases::Editor;

use crate::view_models::TabViewModel;

pub fn present(editor: &Editor) -> Vec<TabViewModel> {
    let state = editor.state();
    state
        .documents
        .iter()
        .enumerate()
        .map(|(index, doc)| TabViewModel {
            title: doc.name(),
            tooltip: doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| doc.name()),
            dirty: doc.is_dirty(),
            active: index == state.active,
        })
        .collect()
}
