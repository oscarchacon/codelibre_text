use std::time::Duration;

use entities::text::line_col;
use use_cases::Editor;

use crate::view_models::StatusBarViewModel;

/// How long a status message stays visible.
pub const MESSAGE_DURATION: Duration = Duration::from_secs(6);

pub fn present(editor: &Editor) -> StatusBarViewModel {
    let message = editor
        .state()
        .status
        .as_ref()
        .filter(|m| m.shown_at.elapsed() < MESSAGE_DURATION)
        .map(|m| m.text.clone());

    let Some(doc) = editor.active_document() else {
        return StatusBarViewModel {
            position: String::new(),
            message,
            indentation: String::new(),
            line_ending: String::new(),
            encoding: String::new(),
            syntax: String::new(),
        };
    };

    let selection = doc.selection();
    let (line, column) = line_col(&doc.text, selection.head);
    let mut position = format!("Line {}, Column {}", line + 1, column + 1);
    if !selection.is_empty() {
        let n = selection.len();
        position.push_str(&format!("; {n} character{} selected", if n == 1 { "" } else { "s" }));
    }

    let settings = editor.view_settings();
    let tab_size = settings.usize("tab_size", 4);
    StatusBarViewModel {
        position,
        message,
        indentation: if settings.bool("translate_tabs_to_spaces", false) {
            format!("Spaces: {tab_size}")
        } else {
            format!("Tab Size: {tab_size}")
        },
        line_ending: doc.line_ending.label().to_string(),
        encoding: doc.encoding.label().to_string(),
        syntax: doc.syntax.clone(),
    }
}
