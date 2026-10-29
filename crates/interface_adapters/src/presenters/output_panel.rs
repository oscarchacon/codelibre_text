use use_cases::Editor;

use crate::view_models::OutputPanelViewModel;

/// Only the end of a long output is drawn (a label with every line of a big build would be slow).
pub const MAX_LINES: usize = 2000;

pub fn present(editor: &Editor) -> Option<OutputPanelViewModel> {
    let panel = editor.shown_output_panel()?;
    let text = &panel.document.text;
    let start = text
        .rmatch_indices('\n')
        .nth(MAX_LINES)
        .map_or(0, |(i, _)| i + 1);
    Some(OutputPanelViewModel {
        name: panel.name.clone(),
        text: text[start..].to_string(),
        font_size: editor.global_settings().f32("font_size", 11.0),
    })
}
