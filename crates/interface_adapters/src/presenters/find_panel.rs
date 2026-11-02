use use_cases::Editor;

use crate::view_models::FindPanelViewModel;
use crate::view_state::ViewState;

pub fn present(editor: &mut Editor, view: &mut ViewState) -> Option<FindPanelViewModel> {
    if !view.find_panel.visible {
        return None;
    }
    let match_count = editor.find_matches().len();
    let state = editor.state();
    let find = &state.find;
    let (status, has_error) = match (&state.find_error, find.query.is_empty()) {
        (Some(error), _) => (editor.tr_args("Error: {error}", &[("error", error)]), true),
        (None, true) => (String::new(), false),
        (None, false) => (editor.tr_n("{count} match", "{count} matches", match_count, &[]), false),
    };
    Some(FindPanelViewModel {
        query: find.query.clone(),
        replacement: state.replacement.clone(),
        replace_mode: view.find_panel.replace_mode,
        regex: find.regex,
        case_sensitive: find.case_sensitive,
        whole_word: find.whole_word,
        wrap: find.wrap,
        status,
        has_error,
        focus_requested: std::mem::take(&mut view.find_panel.focus_requested),
    })
}
