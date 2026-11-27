use use_cases::Editor;

use crate::presenters::{editor as editor_presenter, tabs};
use crate::view_models::GroupViewModel;
use crate::view_state::ViewState;

/// Every group of the window's layout, with its tabs and the documents it shows (its selected
/// tabs, side by side).
pub fn present(editor: &mut Editor, view: &mut ViewState) -> Vec<GroupViewModel> {
    let active_group = editor.active_group();
    (0..editor.group_count())
        .map(|group| {
            let rect = editor
                .layout()
                .cell_rect(group)
                .unwrap_or([0.0, 0.0, 1.0, 1.0])
                .map(|f| f as f32);
            let shown = editor.selected_indexes_in_group(group);
            GroupViewModel {
                group,
                rect,
                active: group == active_group,
                show_tabs: view.show_tabs,
                tabs: tabs::present_group(editor, group),
                editors: shown
                    .into_iter()
                    .filter_map(|index| editor_presenter::present_document(editor, view, index))
                    .collect(),
            }
        })
        .collect()
}
