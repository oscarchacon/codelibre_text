use use_cases::search::SearchKind;
use use_cases::Editor;

use crate::view_models::{InputPanelViewModel, QuickPanelItemViewModel, QuickPanelViewModel};
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
            SearchKind::PluginItems => editor.plugin_quick_panel().map_or("", |p| p.placeholder.as_str()),
        }
        .to_string(),
        no_matches: editor.tr("No matches").to_string(),
        query: panel.query.clone(),
        selected: panel.selected,
        take_focus: std::mem::take(&mut panel.focus_requested),
        monospace: panel.kind == SearchKind::PluginItems && editor.plugin_quick_panel().is_some_and(|p| p.monospace),
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

/// The input panel a plugin opened.
pub fn present_input_panel(view: &mut ViewState) -> Option<InputPanelViewModel> {
    let panel = view.input_panel.as_mut()?;
    Some(InputPanelViewModel {
        caption: panel.caption.clone(),
        text: panel.text.clone(),
        take_focus: std::mem::take(&mut panel.focus_requested),
    })
}
