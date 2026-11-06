//! Use cases: the clipboard history of `paste_from_history` (Sublime's Edit > Paste from History):
//! what copy and cut put on the clipboard, newest first, and what plugins add to it (Terminus,
//! through `Default.paste_from_history.g_clipboard_history`).

use entities::text::selected_text;

use crate::editor::Editor;

/// As many texts as Sublime's `ClipboardHistory.LIST_LIMIT`.
pub const CLIPBOARD_HISTORY_LIMIT: usize = 15;
/// The length of an entry's caption (Sublime's `CAPTION_LENGTH`).
const CAPTION_LENGTH: usize = 45;

impl Editor {
    /// The texts copied, newest first.
    pub fn clipboard_history(&self) -> &[String] {
        &self.state.clipboard_history
    }

    /// Puts `text` first in the history, once (an earlier copy of it goes).
    pub fn push_clipboard_history(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let history = &mut self.state.clipboard_history;
        history.retain(|entry| entry != text);
        history.insert(0, text.to_string());
        history.truncate(CLIPBOARD_HISTORY_LIMIT);
    }

    /// `copy` / `cut` of the selections of the active document (the text widget copies them, one
    /// per line, as Sublime does): what reaches the clipboard goes to the history.
    pub fn remember_copied_selections(&mut self) {
        let Some(doc) = self.active_document() else { return };
        let selections: Vec<_> = doc.selections().into_iter().filter(|s| !s.is_empty()).collect();
        let text = selections.iter().map(|&s| selected_text(&doc.text, s)).collect::<Vec<_>>().join("\n");
        self.push_clipboard_history(&text);
    }

    /// One line per entry for the quick panel: the text without line breaks or leading blanks,
    /// cut at 45 chars, as Sublime's captions.
    pub fn clipboard_history_captions(&self) -> Vec<String> {
        let caption = |text: &str| {
            let flat: String = text.replace(['\n', '\r'], "");
            let flat = flat.trim_start();
            if flat.chars().count() > CAPTION_LENGTH {
                format!("{}...", flat.chars().take(CAPTION_LENGTH).collect::<String>())
            } else {
                flat.to_string()
            }
        };
        self.state.clipboard_history.iter().map(|text| caption(text)).collect()
    }

    /// The entry chosen in Paste from History: it goes back on the clipboard and first in the
    /// history; its text, for the caller to paste. None when there is no such entry.
    pub fn take_clipboard_history_entry(&mut self, index: usize) -> Option<String> {
        let text = self.state.clipboard_history.get(index)?.clone();
        if let Err(e) = self.gateways.clipboard.write_text(&text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return None;
        }
        self.state.line_clipboard = None;
        self.push_clipboard_history(&text);
        Some(text)
    }
}
