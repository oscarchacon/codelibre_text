use use_cases::Editor;

use crate::view_models::TabViewModel;

/// The tabs of the focused group.
pub fn present(editor: &Editor) -> Vec<TabViewModel> {
    present_group(editor, editor.active_group())
}

/// The tabs of one group; the active one is the document the group shows.
pub fn present_group(editor: &Editor, group: usize) -> Vec<TabViewModel> {
    let state = editor.state();
    let shown = editor.active_index_in_group(group);
    editor
        .group_indexes(group)
        .into_iter()
        .map(|index| {
            let doc = &state.documents[index];
            TabViewModel {
                index,
                title: editor.document_name(doc),
                tooltip: doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| editor.document_name(doc)),
                dirty: doc.is_dirty(),
                active: Some(index) == shown,
                transient: doc.transient,
            }
        })
        .collect()
}
