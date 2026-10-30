//! Use cases: text commands on the active document.

use entities::command::Command;
use entities::document::DocumentId;
use entities::selection::Selection;
use entities::text::{line_to_char, word_at};
use entities::text::{byte_to_char, char_to_byte, line_end, line_start};
use entities::text_commands::{self as tc, Case, CaretMotion, LineOrder, TrimOptions};

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
    ExpandSelection(Expansion),
    SelectWord,
    Indent,
    Unindent,
    /// `whole_file`: with no selection, every line instead of the caret's; `force`: also lines
    /// already indented.
    Reindent { whole_file: bool, force: bool },
    /// Tab key: indent lines or insert a tab / spaces.
    InsertTab,
    /// Enter key with `auto_indent`.
    NewlineAutoIndent,
    JoinLines,
    ChangeCase(Case),
    Transpose,
    ConvertIndentation { to_spaces: bool },
    /// `only_modified`: only lines changed since the last save; `trim_on_caret`: also the caret's line.
    TrimTrailingWhitespace { only_modified: bool, trim_on_caret: bool },
    ReorderLines(LineOrder),
    Insert(String),
}

/// The `to` values of `expand_selection` besides `line` and `word`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expansion {
    /// `line_prev`
    LineUpward,
    /// `line_without_eol` / `line_strict`
    LineContent,
    Brackets,
    Indentation,
    Smart,
}

/// Built-in commands that work on any view, also the panels', which are not the active document.
pub(crate) const VIEW_COMMANDS: &[&str] = &["move", "move_to", "left_delete", "right_delete", "select_all", "insert"];

impl Editor {
    /// [`VIEW_COMMANDS`] on the active view; `move` and `move_to` work on every selection. The
    /// widget does the keys bound to them: these are for plugins. Problems go to the status bar.
    pub fn run_view_command_on_active(&mut self, command: &Command) {
        let Some(id) = self.active_document().map(|d| d.id) else {
            return;
        };
        if let Err(message) = self.run_view_command(id, command) {
            self.notify(message);
        }
    }

    pub(crate) fn run_view_command(&mut self, id: DocumentId, command: &Command) -> Result<(), String> {
        let name = command.name.as_str();
        let extend = command.bool_arg("extend", false);
        let target = command.str_arg(if name == "move" { "by" } else { "to" }).to_string();
        let doc = self.any_document_mut(id).ok_or("the view is closed")?;
        match name {
            "select_all" => doc.set_selection(Selection::new(0, doc.char_count())),
            "insert" => {
                let selection = doc.selection();
                doc.replace_range(selection.start(), selection.end(), command.str_arg("characters"));
            }
            "left_delete" | "right_delete" => {
                doc.edit_all(|text, selections| tc::delete_at_selections(text, selections, name == "right_delete"));
            }
            _ => {
                let forward = command.bool_arg("forward", true);
                let motion = |text: &str, selection: Selection| match name {
                    "move" => tc::move_by(text, selection, &target, forward, extend),
                    _ => tc::move_to(text, selection, &target, extend),
                };
                let primary = motion(&doc.text, doc.selection());
                let others: Option<Vec<Selection>> = doc.extra_selections().iter().map(|s| motion(&doc.text, *s)).collect();
                match (primary, others) {
                    (Some(primary), Some(others)) => doc.set_selections(primary, others),
                    _ => return Err(self.tr_args("{command} to {target} is not supported", &[("command", &name), ("target", &target)])),
                }
            }
        }
        Ok(())
    }

    /// Runs a text command on the active document with its settings (tab size, spaces…).
    pub fn run_text_command(&mut self, command: TextCommand) {
        let settings = self.view_settings();
        let tab_size = settings.usize("tab_size", 4).max(1);
        let spaces = settings.bool("translate_tabs_to_spaces", false);
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let syntax = doc.syntax.clone();
        let saved = doc.saved_text().to_string();
        doc.edit(|text, sel| match command {
            TextCommand::ToggleComment { block } => tc::toggle_comment(text, sel, &syntax, block),
            TextCommand::DuplicateLine => Some(tc::duplicate_line(text, sel)),
            TextCommand::SwapLine { up } => tc::swap_line(text, sel, up),
            TextCommand::DeleteLine => Some(tc::delete_line(text, sel)),
            TextCommand::AddLine { before } => Some(tc::add_line(text, sel, before)),
            TextCommand::DeleteToLineEdge { to_end } => Some(tc::delete_to_line_edge(text, sel, to_end)),
            TextCommand::ExpandSelectionToLine => Some(tc::expand_selection_to_line(text, sel)),
            TextCommand::ExpandSelection(to) => Some(match to {
                Expansion::LineUpward => tc::expand_selection_to_line_upward(text, sel),
                Expansion::LineContent => tc::expand_selection_to_line_content(text, sel),
                Expansion::Brackets => tc::expand_selection_to_brackets(text, sel),
                Expansion::Indentation => tc::expand_selection_to_indentation(text, sel, tab_size),
                Expansion::Smart => tc::expand_selection_smart(text, sel, tab_size),
            }),
            TextCommand::SelectWord => Some(word_at(text, sel.head)),
            TextCommand::Indent => Some(tc::indent(text, sel, tab_size, spaces)),
            TextCommand::Unindent => Some(tc::unindent(text, sel, tab_size)),
            TextCommand::Reindent { whole_file, force } => tc::reindent(text, sel, &syntax, (tab_size, spaces), whole_file, force),
            TextCommand::InsertTab => Some(tc::insert_tab(text, sel, tab_size, spaces)),
            TextCommand::NewlineAutoIndent => Some(tc::newline_auto_indent(text, sel, tab_size, spaces)),
            TextCommand::JoinLines => Some(tc::join_lines(text, sel)),
            TextCommand::ChangeCase(case) => Some(tc::change_case(text, sel, case)),
            TextCommand::Transpose => tc::transpose(text, sel),
            TextCommand::ConvertIndentation { to_spaces } => {
                Some(tc::convert_indentation(text, sel, tab_size, to_spaces))
            }
            TextCommand::TrimTrailingWhitespace { only_modified, trim_on_caret } => {
                let options = TrimOptions { only_modified_since: only_modified.then_some(saved.as_str()), trim_on_caret };
                Some(tc::trim_trailing_whitespace_with(text, sel, &options))
            }
            TextCommand::ReorderLines(order) => Some(tc::reorder_lines(text, sel, order)),
            TextCommand::Insert(characters) => Some(tc::replace_selection(text, sel, &characters)),
        });
    }

    /// Sublime's `reindent`; it says so when the syntax has no indentation rules.
    pub fn reindent(&mut self, single_line: bool, force: bool) {
        match self.active_document().map(|d| d.syntax.clone()) {
            Some(syntax) if !tc::can_reindent(&syntax) => {
                self.notify(self.tr_args("No indentation rules for {syntax}", &[("syntax", &syntax)]));
            }
            Some(_) => self.run_text_command(TextCommand::Reindent { whole_file: !single_line, force }),
            None => {}
        }
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
        self.go_to_position(line, 0);
    }

    /// Moves the caret to a zero-based line and column (clamped to the line's length).
    pub fn go_to_position(&mut self, line: usize, column: usize) {
        if let Some(doc) = self.active_document_mut() {
            let start = line_to_char(&doc.text, line);
            let start_byte = char_to_byte(&doc.text, start);
            let length = doc.text[start_byte..line_end(&doc.text, start_byte)].chars().count();
            doc.set_selection(Selection::caret(start + column.min(length)));
        }
    }

    /// With `copy_with_empty_selection` (on by default, as in Sublime), copy and cut with nothing
    /// selected take the whole line.
    pub fn copies_whole_line(&self) -> bool {
        self.view_settings().bool("copy_with_empty_selection", true)
            && self
                .active_document()
                .is_some_and(|d| d.selection().is_empty() && d.extra_selections().is_empty())
    }

    /// `copy` / `cut` of the caret's line (see [`Self::copies_whole_line`]).
    pub fn copy_whole_line(&mut self, cut: bool) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let caret = char_to_byte(&doc.text, doc.selection().head);
        let (start, end) = (line_start(&doc.text, caret), line_end(&doc.text, caret));
        let line = format!("{}\n", &doc.text[start..end]);
        if let Err(e) = self.gateways.clipboard.write_text(&line) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.state.line_clipboard = Some(line);
        if cut {
            self.run_text_command(TextCommand::DeleteLine);
        }
    }

    /// Pasting a line copied with nothing selected inserts it above the caret's line, as in
    /// Sublime. Returns false when the paste is an ordinary one (left to the text widget).
    pub fn pastes_whole_line(&mut self) -> bool {
        let Some(line) = self.state.line_clipboard.clone() else {
            return false;
        };
        let empty_selection = self
            .active_document()
            .is_some_and(|d| d.selection().is_empty() && d.extra_selections().is_empty());
        empty_selection && self.gateways.clipboard.read_text().is_ok_and(|text| text == line)
    }

    /// `paste` of a whole line (see [`Self::pastes_whole_line`]); the caret stays where it was.
    pub fn paste_whole_line(&mut self) {
        let Some(line) = self.state.line_clipboard.clone() else {
            return;
        };
        if let Some(doc) = self.active_document_mut() {
            let caret = char_to_byte(&doc.text, doc.selection().head);
            let start = byte_to_char(&doc.text, line_start(&doc.text, caret));
            doc.replace_range(start, start, &line);
        }
    }

    /// Text to paste from the clipboard (the view inserts it, so it joins the widget's undo history).
    pub fn clipboard_text(&mut self) -> Option<String> {
        match self.gateways.clipboard.read_text() {
            Ok(text) => Some(text),
            Err(e) => {
                self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
                None
            }
        }
    }
}
