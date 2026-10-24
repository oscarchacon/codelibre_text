//! Use cases: text commands on the active document.

use entities::selection::Selection;
use entities::text::{line_to_char, word_at};
use entities::text_commands::{self as tc, Case, LineOrder};

use crate::editor::Editor;

/// The text commands the editor offers. Each one maps to a function in
/// `entities::text_commands`; this enum is what controllers ask for.
pub enum TextCommand {
    ToggleComment { block: bool },
    DuplicateLine,
    SwapLine { up: bool },
    DeleteLine,
    AddLine { before: bool },
    DeleteToLineEdge { to_end: bool },
    ExpandSelectionToLine,
    SelectWord,
    Indent,
    Unindent,
    /// Tab key: indent lines or insert a tab / spaces.
    InsertTab,
    /// Enter key with `auto_indent`.
    NewlineAutoIndent,
    JoinLines,
    ChangeCase(Case),
    Transpose,
    ConvertIndentation { to_spaces: bool },
    TrimTrailingWhitespace,
    ReorderLines(LineOrder),
    Insert(String),
}

impl Editor {
    /// Runs a text command on the active document with its settings (tab size, spaces…).
    pub fn run_text_command(&mut self, command: TextCommand) {
        let settings = self.view_settings();
        let tab_size = settings.usize("tab_size", 4).max(1);
        let spaces = settings.bool("translate_tabs_to_spaces", false);
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let syntax = doc.syntax.clone();
        doc.edit(|text, sel| match command {
            TextCommand::ToggleComment { block } => tc::toggle_comment(text, sel, &syntax, block),
            TextCommand::DuplicateLine => Some(tc::duplicate_line(text, sel)),
            TextCommand::SwapLine { up } => tc::swap_line(text, sel, up),
            TextCommand::DeleteLine => Some(tc::delete_line(text, sel)),
            TextCommand::AddLine { before } => Some(tc::add_line(text, sel, before)),
            TextCommand::DeleteToLineEdge { to_end } => Some(tc::delete_to_line_edge(text, sel, to_end)),
            TextCommand::ExpandSelectionToLine => Some(tc::expand_selection_to_line(text, sel)),
            TextCommand::SelectWord => Some(word_at(text, sel.head)),
            TextCommand::Indent => Some(tc::indent(text, sel, tab_size, spaces)),
            TextCommand::Unindent => Some(tc::unindent(text, sel, tab_size)),
            TextCommand::InsertTab => Some(tc::insert_tab(text, sel, tab_size, spaces)),
            TextCommand::NewlineAutoIndent => Some(tc::newline_auto_indent(text, sel, tab_size, spaces)),
            TextCommand::JoinLines => Some(tc::join_lines(text, sel)),
            TextCommand::ChangeCase(case) => Some(tc::change_case(text, sel, case)),
            TextCommand::Transpose => tc::transpose(text, sel),
            TextCommand::ConvertIndentation { to_spaces } => {
                Some(tc::convert_indentation(text, sel, tab_size, to_spaces))
            }
            TextCommand::TrimTrailingWhitespace => Some(tc::trim_trailing_whitespace(text, sel)),
            TextCommand::ReorderLines(order) => Some(tc::reorder_lines(text, sel, order)),
            TextCommand::Insert(characters) => Some(tc::replace_selection(text, sel, &characters)),
        });
    }

    /// Moves the caret to the start of a zero-based line.
    pub fn go_to_line(&mut self, line: usize) {
        if let Some(doc) = self.active_document_mut() {
            let position = line_to_char(&doc.text, line);
            doc.set_selection(Selection::caret(position));
        }
    }

    /// Text to paste from the clipboard (the view inserts it, so it joins the widget's undo history).
    pub fn clipboard_text(&mut self) -> Option<String> {
        match self.gateways.clipboard.read_text() {
            Ok(text) => Some(text),
            Err(e) => {
                self.notify(format!("Clipboard unavailable: {e}"));
                None
            }
        }
    }
}
