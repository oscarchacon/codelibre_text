use entities::text::char_to_byte;
use use_cases::Editor;

use crate::view_models::{OutputPanelHighlight, OutputPanelViewModel};

/// Only the end of a long output is drawn (a label with every line of a big build would be slow).
pub const MAX_LINES: usize = 2000;

/// A view's syntax when nothing assigned one.
const PLAIN_TEXT: &str = "Plain Text";

pub fn present(editor: &Editor) -> Option<OutputPanelViewModel> {
    let panel = editor.shown_output_panel()?;
    let document = &panel.document;
    let text = &document.text;
    let start = text
        .rmatch_indices('\n')
        .nth(MAX_LINES)
        .map_or(0, |(i, _)| i + 1);
    let first_line = text[..start].matches('\n').count();
    let settings = editor.settings_of(document.id);
    let highlight = (!document.syntax.is_empty() && document.syntax != PLAIN_TEXT).then(|| OutputPanelHighlight {
        document: document.id,
        syntax: document.syntax.clone(),
        color_scheme: editor.color_scheme_name(),
        full_text: text.clone(),
        start,
    });
    // Selections are chars of the whole text; the panel shows bytes from `start`.
    let selections = document
        .selections()
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(|s| char_to_byte(text, s.start()).max(start) - start..char_to_byte(text, s.end()).max(start) - start)
        .filter(|range| !range.is_empty())
        .collect();
    Some(OutputPanelViewModel {
        name: panel.name.clone(),
        text: text[start..].to_string(),
        font_size: settings.f32("font_size", 11.0),
        word_wrap: settings.bool("word_wrap", true),
        first_line,
        current_line: panel.current_result.and_then(|line| line.checked_sub(first_line)),
        result_revision: panel.result_revision,
        highlight,
        selections,
    })
}
