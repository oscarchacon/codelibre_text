//! Use cases: text commands on the active document.

use entities::selection::Selection;
use entities::text::{line_to_char, word_at};
use entities::text_commands::{self as tc, Case, CaretMotion, LineOrder};

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

    /// Typing while there are several selections (Ctrl+D): the text goes into every one.
    pub fn insert_at_selections(&mut self, insert: &str) {
        let settings = self.view_settings();
        let insert = if insert == "\t" && settings.bool("translate_tabs_to_spaces", false) {
            " ".repeat(settings.usize("tab_size", 4).max(1))
        } else {
            insert.to_string()
        };
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|text, selections| tc::replace_selections(text, selections, &insert));
        }
    }

    /// Backspace (Delete when `forward`) at every selection.
    pub fn delete_at_selections(&mut self, forward: bool) {
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|text, selections| tc::delete_at_selections(text, selections, forward));
        }
    }

    /// Arrow keys, Home and End while there are several selections: every caret moves.
    pub fn move_carets(&mut self, motion: CaretMotion) {
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|text, selections| tc::move_carets(text, selections, motion));
        }
    }

    /// `select_lines` (Ctrl+Alt+Up/Down): adds a caret on the line above or below the selections.
    /// The newest caret becomes the primary one, so the view scrolls to it.
    pub fn add_carets_on_next_line(&mut self, forward: bool) {
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let selections = doc.selections();
        let added = tc::carets_on_next_line(&doc.text, &selections, forward);
        if let Some(&primary) = added.last() {
            doc.set_selections(primary, selections.into_iter().chain(added));
        }
    }

    /// `split_selection_into_lines` (Ctrl+Shift+L): one selection per selected line.
    pub fn split_selection_into_lines(&mut self) {
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let primary = doc.selection();
        let mut pieces = tc::split_into_lines(&doc.text, primary);
        // The primary one stays on the line where the caret was.
        let new_primary = if primary.head < primary.anchor { pieces[0] } else { pieces[pieces.len() - 1] };
        for &extra in doc.extra_selections() {
            pieces.extend(tc::split_into_lines(&doc.text, extra));
        }
        doc.set_selections(new_primary, pieces);
    }

    /// `single_selection` (Escape): keeps only the primary selection.
    pub fn single_selection(&mut self) {
        if let Some(doc) = self.active_document_mut() {
            doc.single_selection();
        }
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
