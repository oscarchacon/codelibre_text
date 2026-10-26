use use_cases::search::SearchKind;
use use_cases::Editor;

use crate::view_models::{QuickPanelItemViewModel, QuickPanelViewModel};
use crate::view_state::ViewState;

/// The open quick panel, searching again only when the query changed.
pub fn present(editor: &Editor, view: &mut ViewState) -> Option<QuickPanelViewModel> {
    let panel = view.quick_panel.as_mut()?;
    if panel.results_query.as_deref() != Some(panel.query.as_str()) {
        panel.results = editor.search(panel.kind, &panel.query);
        panel.results_query = Some(panel.query.clone());
        panel.selected = panel.selected.min(panel.results.len().saturating_sub(1));
    }
    Some(QuickPanelViewModel {
        placeholder: match panel.kind {
            SearchKind::Commands => editor.tr("Command Palette"),
            SearchKind::Goto => editor.tr("Goto Anything — file name, :line, @symbol"),
            SearchKind::ColorSchemes => editor.tr("Select Color Scheme"),
            SearchKind::Languages => editor.tr("Select Language"),
        }
        .to_string(),
        no_matches: editor.tr("No matches").to_string(),
        query: panel.query.clone(),
        selected: panel.selected,
        take_focus: std::mem::take(&mut panel.focus_requested),
        items: panel
            .results
            .iter()
            .map(|c| QuickPanelItemViewModel {
                caption: c.caption.clone(),
                detail: c.detail.clone(),
                hint: c.hint.clone(),
            })
            .collect(),
    })
}
