//! Use cases: text commands on the active document.

use entities::command::Command;
use entities::document::DocumentId;
use entities::selection::Selection;
use entities::text::{line_to_char, selected_text, word_at};
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
    /// `insert_snippet`'s `contents` (see `text_commands::insert_snippet`).
    InsertSnippet(String),
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
pub(crate) const VIEW_COMMANDS: &[&str] = &["move", "move_to", "left_delete", "right_delete", "delete_word", "select_all", "insert"];

/// Built-in commands that only change a view's text and selections: in a panel's view they run
/// with the panel as the active view for a moment (see `Editor::run_in_panel`). Not those that
/// open, close or save views, show panels, or go through the text widget (undo, the clipboard).
pub(crate) const PANEL_TEXT_COMMANDS: &[&str] = &[
    "toggle_comment", "duplicate_line", "swap_line_up", "swap_line_down", "expand_selection", "indent", "unindent", "reindent",
    "join_lines", "upper_case", "lower_case", "title_case", "swap_case", "transpose", "expand_tabs", "unexpand_tabs",
    "trim_trailing_white_space", "sort_lines", "permute_lines", "run_macro_file", "insert_snippet", "goto_line",
    "find_under_expand", "find_all_under", "select_lines", "split_selection_into_lines", "single_selection", "next_field",
    "prev_field", "clear_fields", "set_file_type", "set_line_ending",
];

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
        let settings = self.settings_of(id);
        let doc = self.any_document_mut(id).ok_or("the view is closed")?;
        match name {
            "select_all" => doc.set_selection(Selection::new(0, doc.char_count())),
            "insert" => {
                // Typing goes into every selection; Tab follows the indentation settings.
                let characters = command.str_arg("characters");
                let insert = if characters == "\t" && settings.bool("translate_tabs_to_spaces", false) {
                    " ".repeat(settings.usize("tab_size", 4).max(1))
                } else {
                    characters.to_string()
                };
                doc.edit_all(|text, selections| tc::replace_selections(text, selections, &insert));
            }
            "left_delete" | "right_delete" => {
                doc.edit_all(|text, selections| tc::delete_at_selections(text, selections, name == "right_delete"));
            }
            "delete_word" => {
                // Ctrl+Backspace / Ctrl+Delete: `forward` deletes the word after the caret.
                let forward = command.bool_arg("forward", false);
                doc.edit_all(|text, selections| tc::delete_word_at_selections(text, selections, forward));
            }
            _ => {
                let (start, end) = doc.visible_range();
                let shown = doc.text.chars().skip(start).take(end - start).filter(|&c| c == '\n').count();
                let flag = |key: &str| command.bool_arg(key, false);
                let how = tc::MoveBy {
                    by: target.clone(),
                    forward: command.bool_arg("forward", true),
                    extend,
                    stops: tc::Stops {
                        word_begin: flag("word_begin"),
                        word_end: flag("word_end"),
                        punct_begin: flag("punct_begin"),
                        punct_end: flag("punct_end"),
                        sub_word_begin: flag("sub_word_begin"),
                        sub_word_end: flag("sub_word_end"),
                        line_begin: flag("line_begin"),
                        line_end: flag("line_end"),
                        empty_line: flag("empty_line"),
                    },
                    // A page leaves one line of the last one in sight, as Page Down does.
                    page_lines: shown.saturating_sub(1).max(1),
                    separators: command.args.get("separators").and_then(|v| v.as_str()).map(String::from),
                };
                let motion = |text: &str, selection: Selection| match name {
                    "move" => tc::move_by(text, selection, &how),
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
        if let TextCommand::InsertSnippet(contents) = &command {
            self.insert_snippet(contents);
            return;
        }
        // Typing edits every selection, as the text widget does while there are several.
        if let TextCommand::Insert(characters) = &command {
            self.insert_at_selections(characters);
            return;
        }
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
            TextCommand::InsertSnippet(_) => unreachable!("snippets are inserted above"),
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
        self.push_clipboard_history(&line);
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

    /// `copy` / `cut` without the text widget, for a plugin's command during an event (the widget
    /// does them otherwise): the selections, one per line, go to the clipboard and its history;
    /// `cut` deletes them. With nothing selected it does nothing (see [`Self::copy_whole_line`]).
    pub fn copy_selections(&mut self, cut: bool) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let selections: Vec<Selection> = doc.selections().into_iter().filter(|s| !s.is_empty()).collect();
        if selections.is_empty() {
            return;
        }
        let text = selections.iter().map(|&s| selected_text(&doc.text, s)).collect::<Vec<_>>().join("\n");
        if let Err(e) = self.gateways.clipboard.write_text(&text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.state.line_clipboard = None;
        self.push_clipboard_history(&text);
        if cut {
            if let Some(doc) = self.active_document_mut() {
                doc.edit_all(|text, selections| tc::replace_selections(text, selections, ""));
            }
        }
    }

    /// `paste` without the text widget, for a plugin's command during an event: the clipboard's
    /// text replaces every selection.
    pub fn paste_at_selections(&mut self) {
        let Some(text) = self.clipboard_text() else {
            return;
        };
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|content, selections| tc::replace_selections(content, selections, &text));
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
