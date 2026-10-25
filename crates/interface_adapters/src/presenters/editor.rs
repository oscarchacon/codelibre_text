use serde_json::Value;
use use_cases::Editor;

use crate::view_models::EditorViewModel;
use crate::view_state::ViewState;

/// The active document as the text area should show it.
pub fn present(editor: &mut Editor, view: &mut ViewState) -> Option<EditorViewModel> {
    let find_matches = if view.find_panel.visible { editor.find_matches() } else { Vec::new() };
    let settings = editor.view_settings();
    let color_scheme = editor.color_scheme_name();
    let doc = editor.active_document()?;

    // The text takes the keyboard focus unless a panel or dialog has it.
    let take_focus = std::mem::take(&mut view.focus_editor)
        && view.quick_panel.is_none()
        && !view.find_panel.focus_requested
        && editor.state().pending_close.is_none();

    Some(EditorViewModel {
        document: doc.id,
        read_only: doc.read_only,
        syntax: doc.syntax.clone(),
        color_scheme,
        selection: doc.selection(),
        extra_selections: doc.extra_selections().to_vec(),
        selection_revision: doc.selection_revision(),
        font_size: settings.f32("font_size", 11.0).clamp(4.0, 72.0),
        tab_size: settings.usize("tab_size", 4).max(1),
        word_wrap: settings.get("word_wrap") == Some(&Value::Bool(true)),
        line_numbers: settings.bool("line_numbers", true),
        highlight_line: settings.bool("highlight_line", true),
        scroll_past_end: settings.bool("scroll_past_end", true),
        rulers: settings.numbers("rulers"),
        auto_indent: settings.bool("auto_indent", true),
        find_matches,
        take_focus,
    })
}
