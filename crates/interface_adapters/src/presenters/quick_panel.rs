use use_cases::search::{CandidateTarget, SearchKind};
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
    let plugin_panel = editor.plugin_quick_panel().filter(|_| panel.kind == SearchKind::PluginItems);
    Some(QuickPanelViewModel {
        placeholder: match panel.kind {
            SearchKind::Commands => editor.tr("Command Palette"),
            SearchKind::Goto => editor.tr("Goto Anything — file name, :line, @symbol"),
            SearchKind::ColorSchemes => editor.tr("Select Color Scheme"),
            SearchKind::Languages => editor.tr("Select Language"),
            SearchKind::BuildSystems => editor.tr("Select Build System"),
            SearchKind::PluginItems => editor.plugin_quick_panel().map_or("", |p| p.placeholder.as_str()),
        }
        .to_string(),
        no_matches: editor.tr("No matches").to_string(),
        query: panel.query.clone(),
        selected: panel.selected,
        take_focus: std::mem::take(&mut panel.focus_requested),
        monospace: plugin_panel.is_some_and(|p| p.monospace),
        keep_open: plugin_panel.is_some_and(|p| p.keep_open),
        items: panel
            .results
            .iter()
            .map(|c| QuickPanelItemViewModel {
                caption: c.caption.clone(),
                detail: c.detail.clone(),
                hint: c.hint.clone(),
                kind: match (&c.target, plugin_panel) {
                    (CandidateTarget::PluginItem(item), Some(plugin)) => plugin.items[*item].kind.clone(),
                    _ => String::new(),
                },
            })
            .collect(),
    })
}

/// The input panel a plugin opened.
pub fn present_input_panel(editor: &Editor, view: &mut ViewState) -> Option<InputPanelViewModel> {
    let state = view.input_panel.as_mut()?;
    let panel = editor.plugin_input_panel()?;
    let selection = panel.document.selection();
    Some(InputPanelViewModel {
        id: panel.document.id,
        caption: panel.caption.clone(),
        text: panel.document.text.clone(),
        selection: (selection.anchor, selection.head),
        selection_revision: panel.document.selection_revision(),
        take_focus: std::mem::take(&mut state.focus_requested),
    })
}
