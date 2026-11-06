use use_cases::Editor;

use crate::presenters::{editor as editor_presenter, tabs};
use crate::view_models::GroupViewModel;
use crate::view_state::ViewState;

/// Every group of the window's layout, with its tabs and the document it shows.
pub fn present(editor: &mut Editor, view: &mut ViewState) -> Vec<GroupViewModel> {
    let active_group = editor.active_group();
    (0..editor.group_count())
        .map(|group| {
            let rect = editor.layout().cell_rect(group).unwrap_or([0.0, 0.0, 1.0, 1.0]).map(|f| f as f32);
            let shown = editor.active_index_in_group(group);
            GroupViewModel {
                group,
                rect,
                active: group == active_group,
                tabs: tabs::present_group(editor, group),
                editor: shown.and_then(|index| editor_presenter::present_document(editor, view, index)),
            }
        })
        .collect()
}
