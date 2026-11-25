//! Use cases: text commands on the active document.

use entities::clipboard;
use entities::command::Command;
use entities::document::DocumentId;
use entities::drag_select as ds;
use entities::selection;
use entities::selection::Selection;
use entities::settings::Settings;
use entities::text::{byte_to_char, char_to_byte, line_end, line_start};
use entities::text::{line_to_char, word_at};
use entities::text_commands::{self as tc, CaretMotion, Case, LineOrder, TrimOptions};

use crate::editor::Editor;

/// The text commands the editor offers. Each one maps to a function in
/// `entities::text_commands`; this enum is what controllers ask for.
pub enum TextCommand {
    ToggleComment {
        block: bool,
    },
    DuplicateLine,
    SwapLine {
        up: bool,
    },
    DeleteLine,
    AddLine {
        before: bool,
    },
    DeleteToLineEdge {
        to_end: bool,
    },
    ExpandSelectionToLine,
    ExpandSelection(Expansion),
    SelectWord,
    Indent,
    Unindent,
    /// `whole_file`: with no selection, every line instead of the caret's; `force`: also lines
    /// already indented.
    Reindent {
        whole_file: bool,
        force: bool,
    },
    /// Tab key: indent lines or insert a tab / spaces.
    InsertTab,
    /// Enter key with `auto_indent`.
    NewlineAutoIndent,
    JoinLines,
    ChangeCase(Case),
    Transpose,
    ConvertIndentation {
        to_spaces: bool,
    },
    /// `only_modified`: only lines changed since the last save; `trim_on_caret`: also the caret's line.
    TrimTrailingWhitespace {
        only_modified: bool,
        trim_on_caret: bool,
    },
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
pub(crate) const VIEW_COMMANDS: &[&str] = &[
    "move",
    "move_to",
    "left_delete",
    "right_delete",
    "delete_word",
    "select_all",
    "insert",
];

/// Built-in commands that only change a view's text and selections: in a panel's view they run
/// with the panel as the active view for a moment (see `Editor::run_in_panel`). Not those that
/// open, close or save views, show panels, or go through the text widget (undo, the clipboard).
pub(crate) const PANEL_TEXT_COMMANDS: &[&str] = &[
    "toggle_comment",
    "duplicate_line",
    "swap_line_up",
    "swap_line_down",
    "expand_selection",
    "indent",
    "unindent",
    "reindent",
    "join_lines",
    "upper_case",
    "lower_case",
    "title_case",
    "swap_case",
    "transpose",
    "expand_tabs",
    "unexpand_tabs",
    "trim_trailing_white_space",
    "sort_lines",
    "permute_lines",
    "run_macro_file",
    "insert_snippet",
    "goto_line",
    "find_under_expand",
    "find_under_expand_skip",
    "find_all_under",
    "select_lines",
    "split_selection_into_lines",
    "single_selection",
    "next_field",
    "prev_field",
    "clear_fields",
    "set_file_type",
    "set_line_ending",
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

    pub(crate) fn run_view_command(
        &mut self,
        id: DocumentId,
        command: &Command,
    ) -> Result<(), String> {
        let name = command.name.as_str();
        let extend = command.bool_arg("extend", false);
        let target = command
            .str_arg(if name == "move" { "by" } else { "to" })
            .to_string();
        let settings = self.settings_of(id);
        let doc = self.any_document_mut(id).ok_or("the view is closed")?;
        match name {
            "select_all" => doc.set_selection(Selection::new(0, doc.char_count())),
            "insert" => {
                // Typing goes into every selection; Tab follows the indentation settings.
                let characters = command.str_arg("characters");
                let insert =
                    if characters == "\t" && settings.bool("translate_tabs_to_spaces", false) {
                        " ".repeat(settings.usize("tab_size", 4).max(1))
                    } else {
                        characters.to_string()
                    };
                doc.edit_all(|text, selections| tc::replace_selections(text, selections, &insert));
            }
            "left_delete" | "right_delete" => {
                doc.edit_all(|text, selections| {
                    tc::delete_at_selections(text, selections, name == "right_delete")
                });
            }
            "delete_word" => {
                // Ctrl+Backspace / Ctrl+Delete: `forward` deletes the word after the caret.
                let forward = command.bool_arg("forward", false);
                doc.edit_all(|text, selections| {
                    tc::delete_word_at_selections(text, selections, forward)
                });
            }
            _ => {
                let (start, end) = doc.visible_range();
                let shown = doc
                    .text
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .filter(|&c| c == '\n')
                    .count();
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
                    separators: separators_of(&settings, |key| {
                        command.args.get(key).and_then(|v| v.as_str())
                    }),
                    limit_on_up_down: settings.bool("move_to_limit_on_up_down", false),
                };
                if name == "move" && matches!(target.as_str(), "lines" | "pages") {
                    // Every caret keeps the screen column it aimed for (Sublime's `xpos`).
                    let step = tc::VerticalMove {
                        lines: if target == "lines" { 1 } else { how.page_lines },
                        forward: how.forward,
                        extend,
                        to_limit: target == "pages" || how.limit_on_up_down,
                        tab_size: settings.usize("tab_size", 4).max(1),
                    };
                    let rows = ds::Rows::of(&doc.text, doc.geometry());
                    let mut goals = Vec::new();
                    let mut moved: Vec<Selection> = std::iter::once(doc.selection())
                        .chain(doc.extra_selections().iter().copied())
                        .map(|selection| {
                            let goal = doc.goal_column(selection);
                            let (moved, goal) =
                                tc::move_vertically(&doc.text, &rows, selection, goal, &step);
                            goals.push((moved, goal));
                            moved
                        })
                        .collect();
                    let primary = moved.remove(0);
                    doc.set_selections(primary, moved);
                    doc.set_goal_columns(goals);
                    return Ok(());
                }
                let geometry = *doc.geometry();
                let motion = |text: &str, selection: Selection| match name {
                    "move" => tc::move_by(text, selection, &how),
                    _ => tc::move_to_in_view(text, selection, &target, extend, &geometry),
                };
                let primary = motion(&doc.text, doc.selection());
                let others: Option<Vec<Selection>> = doc
                    .extra_selections()
                    .iter()
                    .map(|s| motion(&doc.text, *s))
                    .collect();
                match (primary, others) {
                    (Some(primary), Some(others)) => doc.set_selections(primary, others),
                    _ => {
                        return Err(self.tr_args(
                            "{command} to {target} is not supported",
                            &[("command", &name), ("target", &target)],
                        ))
                    }
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
            TextCommand::DeleteToLineEdge { to_end } => {
                Some(tc::delete_to_line_edge(text, sel, to_end))
            }
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
            TextCommand::Reindent { whole_file, force } => {
                tc::reindent(text, sel, &syntax, (tab_size, spaces), whole_file, force)
            }
            TextCommand::InsertTab => Some(tc::insert_tab(text, sel, tab_size, spaces)),
            TextCommand::NewlineAutoIndent => {
                Some(tc::newline_auto_indent(text, sel, tab_size, spaces))
            }
            TextCommand::JoinLines => Some(tc::join_lines(text, sel)),
            TextCommand::ChangeCase(case) => Some(tc::change_case(text, sel, case)),
            TextCommand::Transpose => tc::transpose(text, sel),
            TextCommand::ConvertIndentation { to_spaces } => {
                Some(tc::convert_indentation(text, sel, tab_size, to_spaces))
            }
            TextCommand::TrimTrailingWhitespace {
                only_modified,
                trim_on_caret,
            } => {
                let options = TrimOptions {
                    only_modified_since: only_modified.then_some(saved.as_str()),
                    trim_on_caret,
                };
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
                self.notify(
                    self.tr_args("No indentation rules for {syntax}", &[("syntax", &syntax)]),
                );
            }
            Some(_) => self.run_text_command(TextCommand::Reindent {
                whole_file: !single_line,
                force,
            }),
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
        self.as_widget_command(|editor| {
            if let Some(doc) = editor.active_document_mut() {
                doc.edit_all(|text, selections| tc::replace_selections(text, selections, &insert));
            }
        });
    }

    /// Backspace (Delete when `forward`) at every selection.
    pub fn delete_at_selections(&mut self, forward: bool) {
        self.as_widget_command(|editor| {
            if let Some(doc) = editor.active_document_mut() {
                doc.edit_all(|text, selections| {
                    tc::delete_at_selections(text, selections, forward)
                });
            }
        });
    }

    /// Up (or Down, `forward`) from the first row drawn (or the last) with
    /// `move_to_limit_on_up_down` off: the caret stays, as in Sublime. (The text widget would go
    /// to the start or end of the text, which is what the setting turned on does.)
    pub fn caret_stays_on_up_down(&self, forward: bool) -> bool {
        let Some(doc) = self.active_document() else {
            return false;
        };
        if self
            .settings_of(doc.id)
            .bool("move_to_limit_on_up_down", false)
        {
            return false;
        }
        let text = &doc.text;
        let head = doc.selection().head;
        let (start, end) = doc.geometry().wrapped_row(text, head).unwrap_or_else(|| {
            let at = char_to_byte(text, head);
            (
                byte_to_char(text, line_start(text, at)),
                byte_to_char(text, line_end(text, at)),
            )
        });
        if forward {
            end == doc.char_count()
        } else {
            start == 0
        }
    }

    /// Arrow keys, Home and End while there are several selections: every caret moves.
    pub fn move_carets(&mut self, motion: CaretMotion) {
        self.as_widget_command(|editor| {
            if let Some(doc) = editor.active_document_mut() {
                doc.close_undo_step();
                let geometry = *doc.geometry();
                doc.edit_all(|text, selections| {
                    tc::move_carets(text, selections, motion, &geometry)
                });
            }
        });
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
        let new_primary = if primary.head < primary.anchor {
            pieces[0]
        } else {
            pieces[pieces.len() - 1]
        };
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
            let length = doc.text[start_byte..line_end(&doc.text, start_byte)]
                .chars()
                .count();
            doc.set_selection(Selection::caret(start + column.min(length)));
        }
    }

    /// With `copy_with_empty_selection` (on by default, as in Sublime), copy and cut with nothing
    /// selected (at any caret) take the carets' whole lines.
    pub fn copies_whole_line(&self) -> bool {
        self.view_settings().bool("copy_with_empty_selection", true) && self.nothing_selected()
    }

    /// Whether every selection of the active document is a caret.
    fn nothing_selected(&self) -> bool {
        self.active_document().is_some_and(|d| {
            d.selection().is_empty() && d.extra_selections().iter().all(|s| s.is_empty())
        })
    }

    /// `copy` / `cut` of the carets' lines, each once (see [`Self::copies_whole_line`]); `cut`
    /// deletes them.
    pub fn copy_whole_line(&mut self, cut: bool) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let Some(copied) = clipboard::copied(&doc.text, &doc.selections(), true) else {
            return;
        };
        if let Err(e) = self.gateways.clipboard.write_text(&copied.text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.push_clipboard_history(&copied.text);
        self.state.line_clipboard = Some(copied.text);
        if cut {
            if let Some(doc) = self.active_document_mut() {
                doc.edit_all(clipboard::delete_full_lines);
            }
        }
    }

    /// Pasting lines copied with nothing selected puts them above the carets' lines, as in
    /// Sublime. Returns false when the paste is an ordinary one.
    pub fn pastes_whole_line(&mut self) -> bool {
        let Some(line) = self.state.line_clipboard.clone() else {
            return false;
        };
        self.nothing_selected()
            && self
                .gateways
                .clipboard
                .read_text()
                .is_ok_and(|text| text == line)
    }

    /// `paste` of whole lines (see [`Self::pastes_whole_line`]): above each caret's line, one line
    /// each when there are as many as carets; the carets stay where they were.
    pub fn paste_whole_line(&mut self) {
        let Some(lines) = self.state.line_clipboard.clone() else {
            return;
        };
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|text, selections| {
                let pieces = clipboard::line_pieces(&lines, selections.len());
                clipboard::insert_above_lines(text, selections, &pieces)
            });
        }
    }

    /// `copy` / `cut` without the text widget (with several selections, or for a plugin's command
    /// during an event): the selections, one per line, go to the clipboard and its history; `cut`
    /// deletes them. With nothing selected it does nothing (see [`Self::copy_whole_line`]).
    pub fn copy_selections(&mut self, cut: bool) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let Some(copied) = clipboard::copied(&doc.text, &doc.selections(), false) else {
            return;
        };
        if let Err(e) = self.gateways.clipboard.write_text(&copied.text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.state.line_clipboard = None;
        self.push_clipboard_history(&copied.text);
        if cut {
            if let Some(doc) = self.active_document_mut() {
                doc.edit_all(|text, selections| tc::replace_selections(text, selections, ""));
            }
        }
    }

    /// `paste` without the text widget (with several selections, or for a plugin's command during
    /// an event): one line of the clipboard in each selection when it has as many lines as there
    /// are selections, else all of it in each.
    pub fn paste_at_selections(&mut self) {
        if let Some(text) = self.clipboard_text() {
            self.paste_text_at_selections(&text);
        }
    }

    /// [`Self::paste_at_selections`] of `text` (an entry of Paste from History).
    pub fn paste_text_at_selections(&mut self, text: &str) {
        if let Some(doc) = self.active_document_mut() {
            doc.edit_all(|content, selections| {
                let pieces = clipboard::paste_pieces(text, selections.len());
                tc::replace_selections_each(content, selections, &pieces)
            });
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

/// What separates words and sub-words: `given("separators")` / `given("sub_word_separators")`
/// (a command's or `find_by_class`'s arguments) when not empty, else the view's
/// `word_separators` / `sub_word_separators` settings, as in Sublime.
pub(crate) fn separators_of<'a>(
    settings: &Settings,
    given: impl Fn(&str) -> Option<&'a str>,
) -> tc::Separators {
    let pick = |argument: &str, setting: &str| {
        given(argument)
            .filter(|s| !s.is_empty())
            .or_else(|| settings.str(setting))
            .map(String::from)
    };
    tc::Separators {
        word: pick("separators", "word_separators"),
        sub_word: pick("sub_word_separators", "sub_word_separators"),
    }
}

/// What a mouse drag selects (Sublime's `drag_select` `by`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragBy {
    /// From the press to the pointer.
    #[default]
    Characters,
    /// Whole words (a double click).
    Words,
    /// Whole lines (a triple click).
    Lines,
    /// A column: one selection per line between the press and the pointer.
    Columns,
}

/// How a drag's selections go with those there were when the button went down (Sublime's
/// `drag_select` `additive` / `subtractive`; neither replaces them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragCombine {
    #[default]
    Replace,
    Add,
    Subtract,
}

/// A place the pointer is at in a view's text: the char nearest to it and, for a column drag,
/// how many columns past the end of its row the pointer is (0 over the text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragPoint {
    pub point: usize,
    pub past_end: usize,
    /// The pointer is at or past the end of a row the view wrapped: `point` is that row's end,
    /// which is also where the next row begins.
    pub after_row: bool,
}

impl DragPoint {
    pub fn at(point: usize) -> Self {
        Self {
            point,
            ..Self::default()
        }
    }
}

/// A mouse drag in a view's text, from where the button went down (`anchor`) to where the pointer
/// is now (`head`). `pressed` is its first frame: the selections there are then are the ones the
/// drag adds to or takes from until the button goes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragSelect {
    pub anchor: DragPoint,
    pub head: DragPoint,
    pub by: DragBy,
    pub combine: DragCombine,
    pub pressed: bool,
}

/// The selections of the view a drag is in when its button went down.
#[derive(Debug, Clone, Default)]
pub(crate) struct DragBase {
    pub document: DocumentId,
    pub primary: Selection,
    pub selections: Vec<Selection>,
}

impl Editor {
    /// A mouse drag (Ctrl + click, Alt + click, a column…: Sublime's `drag_select`): the drag's
    /// selections so far replace, join or are taken out of the view's selections at the press.
    /// The one under the pointer becomes the primary selection. Taking out every selection leaves
    /// them as they were, as Sublime keeps at least one.
    pub fn drag_select(&mut self, id: DocumentId, drag: DragSelect) {
        let tab_size = self.settings_of(id).usize("tab_size", 4);
        if drag.pressed {
            self.note_text_clicked();
        }
        let Some(index) = self.index_of(id) else {
            return;
        };
        let doc = &mut self.state.documents[index];
        if drag.pressed {
            doc.close_undo_step();
            self.state.drag_base = Some(DragBase {
                document: id,
                primary: doc.selection(),
                selections: doc.selections(),
            });
        }
        let base = match &self.state.drag_base {
            Some(base) if base.document == id => base.clone(),
            _ => DragBase {
                document: id,
                primary: doc.selection(),
                selections: doc.selections(),
            },
        };
        let text = &doc.text;
        let dragged: Vec<Selection> = match drag.by {
            DragBy::Columns => {
                let rows = ds::Rows::of(text, doc.geometry());
                let at = |p: DragPoint| {
                    let screen_point = if p.after_row {
                        ds::screen_point_after_row
                    } else {
                        ds::screen_point
                    };
                    screen_point(text, &rows, p.point, p.past_end, tab_size)
                };
                ds::column_selections(text, &rows, at(drag.anchor), at(drag.head), tab_size)
            }
            by => {
                let unit = match by {
                    DragBy::Words => ds::DragUnit::Words,
                    DragBy::Lines => ds::DragUnit::Lines,
                    _ => ds::DragUnit::Characters,
                };
                vec![ds::drag_selection(
                    text,
                    drag.anchor.point,
                    drag.head.point,
                    unit,
                )]
            }
        };
        let Some(&last) = dragged.last() else {
            return;
        };
        let (primary, others) = match drag.combine {
            DragCombine::Replace => (last, dragged),
            DragCombine::Add => {
                let mut set = base.selections;
                let mut at = 0;
                for &selection in &dragged {
                    at = selection::add_region(&mut set, selection);
                }
                (set.remove(at), set)
            }
            DragCombine::Subtract => {
                let set = dragged.iter().fold(base.selections, |set, &s| {
                    selection::subtract_region(&set, s)
                });
                if set.is_empty() {
                    (base.primary, Vec::new())
                } else {
                    let primary = set
                        .iter()
                        .copied()
                        .find(|&s| s == base.primary)
                        .unwrap_or(set[set.len() - 1]);
                    (primary, set)
                }
            }
        };
        let others: Vec<Selection> = others.into_iter().filter(|&s| s != primary).collect();
        let unchanged = doc.selection() == primary && {
            let mut now = doc.extra_selections().to_vec();
            let mut then = others.clone();
            now.sort_by_key(|s| (s.start(), s.end()));
            then.sort_by_key(|s| (s.start(), s.end()));
            now == then
        };
        if !unchanged {
            doc.set_selections(primary, others);
            self.end_whole_word_occurrences(id);
            self.unfold_around_carets(id);
        }
    }

    /// The button of a drag went up.
    pub fn end_drag_select(&mut self) {
        self.state.drag_base = None;
    }
}
