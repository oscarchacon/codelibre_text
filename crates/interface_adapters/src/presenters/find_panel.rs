use use_cases::Editor;

use crate::view_models::FindPanelViewModel;
use crate::view_state::ViewState;

pub fn present(editor: &mut Editor, view: &mut ViewState) -> Option<FindPanelViewModel> {
    if !view.find_panel.visible {
        return None;
    }
    let in_files = view.find_panel.in_files;
    // The matches of the active view; Find in Files counts its own in the results.
    let match_count = if in_files {
        0
    } else {
        editor.find_matches().len()
    };
    let state = editor.state();
    let find = &state.find;
    let (status, has_error) = match (&state.find_error, find.query.is_empty()) {
        (Some(error), _) => (editor.tr_args("Error: {error}", &[("error", error)]), true),
        (None, _) if in_files && editor.is_finding_in_files() => {
            (editor.tr("Searching…").to_string(), false)
        }
        (None, true) => (String::new(), false),
        (None, false) if in_files => (String::new(), false),
        (None, false) => (
            editor.tr_n("{count} match", "{count} matches", match_count, &[]),
            false,
        ),
    };
    Some(FindPanelViewModel {
        query: find.query.clone(),
        replacement: state.replacement.clone(),
        replace_mode: view.find_panel.replace_mode,
        in_files,
        where_text: editor.find_where().to_string(),
        regex: find.regex,
        case_sensitive: find.case_sensitive,
        whole_word: find.whole_word,
        wrap: find.wrap,
        use_buffer: editor.find_use_buffer(),
        status,
        has_error,
        focus_requested: std::mem::take(&mut view.find_panel.focus_requested),
    })
}
