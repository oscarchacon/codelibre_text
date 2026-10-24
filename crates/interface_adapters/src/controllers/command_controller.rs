//! Runs a Sublime Text command by name. Key bindings, menus and the command palette all end here,
//! so packages written for Sublime can call these commands with Sublime's arguments.
//!
//! To add a command: add an arm below that calls a use case, then expose it in
//! `assets/Default/` (keymap, menu, command palette).

use entities::command::Command;
use entities::document::LineEnding;
use entities::text_commands::{Case, LineOrder};
use use_cases::editing::TextCommand;
use use_cases::search::SearchKind;
use use_cases::Editor;

use crate::view_state::{QuickPanel, ViewRequest, ViewState, WidgetEdit};

pub fn run(editor: &mut Editor, view: &mut ViewState, command: &Command) {
    match command.name.as_str() {
        // ----- files ---------------------------------------------------------------------------
        "new_file" => editor.new_file(),
        "prompt_open_file" => editor.prompt_open_files(),
        "prompt_open_folder" | "prompt_add_folder" => {
            if let Some(folder) = editor.prompt_add_folder() {
                view.expanded_folders.insert(folder);
                view.show_side_bar = true;
            }
        }
        "open_file" => editor.open_package_file(command.str_arg("file")),
        "open_dir" => editor.open_package_folder(command.str_arg("dir")),
        "save" => {
            editor.save_active(false);
        }
        "prompt_save_as" => {
            editor.save_active(true);
        }
        "save_all" => editor.save_all(),
        "revert" => editor.revert_active_document(),
        "close" | "close_file" => editor.close_active_document(),
        "close_all" => editor.close_all_documents(),
        "close_window" | "exit" => view.request(ViewRequest::CloseWindow),
        "reopen_last_file" => editor.reopen_closed_file(),
        "close_folder_list" => editor.close_all_folders(),
        "refresh_folder_list" => {
            view.forget_folder_listings();
            editor.refresh_file_index();
        }

        // ----- quick panel and panels ----------------------------------------------------------
        "show_overlay" => {
            let kind = match command.str_arg("overlay") {
                "command_palette" => SearchKind::Commands,
                "goto" => SearchKind::Goto,
                other => {
                    editor.notify(format!("Overlay not supported: {other}"));
                    return;
                }
            };
            view.quick_panel = Some(QuickPanel::new(kind, command.str_arg("text").to_string()));
        }
        "hide_overlay" => close_quick_panel(editor, view),
        "select_color_scheme" => {
            let current = editor.color_scheme_name();
            let mut panel = QuickPanel::new(SearchKind::ColorSchemes, String::new());
            panel.results = editor.search(SearchKind::ColorSchemes, "");
            panel.results_query = Some(String::new());
            panel.selected = panel
                .results
                .iter()
                .position(|c| c.caption == current)
                .unwrap_or(0);
            panel.color_scheme_before = Some(current);
            view.quick_panel = Some(panel);
        }
        "show_panel" => {
            let panel = command.str_arg("panel").trim_start_matches("output.");
            match panel {
                "find" | "incremental_find" | "find_in_files" | "replace" => {
                    view.find_panel.visible = true;
                    view.find_panel.replace_mode = panel == "replace";
                    view.find_panel.focus_requested = true;
                    editor.use_selection_as_query_if_single_line();
                }
                other => editor.notify(format!("Panel not supported: {other}")),
            }
        }
        "hide_panel" => {
            view.find_panel.visible = false;
            view.focus_editor = true;
        }

        // ----- find ----------------------------------------------------------------------------
        "find_next" | "find_prev" => find_next(editor, view, command.name == "find_next"),
        "find_under" | "find_under_prev" => {
            editor.use_selection_as_query(true);
            find_next(editor, view, command.name == "find_under");
        }
        "slurp_find_string" => editor.use_selection_as_query(false),
        "find_under_expand" => editor.select_word_or_next_occurrence(),
        "replace_next" => editor.replace_next(),
        "replace_all" => editor.replace_all(),

        // ----- view ----------------------------------------------------------------------------
        "toggle_side_bar" => view.show_side_bar = !view.show_side_bar,
        "toggle_status_bar" => view.show_status_bar = !view.show_status_bar,
        "toggle_full_screen" | "toggle_distraction_free" => {
            view.fullscreen = !view.fullscreen;
            view.request(ViewRequest::SetFullscreen(view.fullscreen));
        }
        "increase_font_size" => editor.change_font_size(1.0),
        "decrease_font_size" => editor.change_font_size(-1.0),
        "reset_font_size" => editor.reset_font_size(),
        "toggle_setting" => editor.toggle_setting(command.str_arg("setting")),
        "set_setting" => {
            let value = command.args.get("value").cloned().unwrap_or_default();
            editor.set_setting(command.str_arg("setting"), value);
        }
        "next_view" => editor.cycle_documents(true),
        "prev_view" => editor.cycle_documents(false),
        "next_view_in_stack" => editor.cycle_recent_documents(true),
        "prev_view_in_stack" => editor.cycle_recent_documents(false),
        "select_by_index" => editor.activate(command.usize_arg("index", 0)),
        "set_file_type" => editor.set_syntax(command.str_arg("syntax")),
        "set_line_ending" => {
            let system = if cfg!(windows) { LineEnding::Windows } else { LineEnding::Unix };
            editor.set_line_ending(LineEnding::from_setting(Some(command.str_arg("type")), system));
        }

        // ----- text ----------------------------------------------------------------------------
        "toggle_comment" => editor.run_text_command(TextCommand::ToggleComment { block: command.bool_arg("block", false) }),
        "duplicate_line" => editor.run_text_command(TextCommand::DuplicateLine),
        "swap_line_up" => editor.run_text_command(TextCommand::SwapLine { up: true }),
        "swap_line_down" => editor.run_text_command(TextCommand::SwapLine { up: false }),
        "expand_selection" => match command.str_arg("to") {
            "line" => editor.run_text_command(TextCommand::ExpandSelectionToLine),
            "word" => editor.run_text_command(TextCommand::SelectWord),
            other => editor.notify(format!("expand_selection to {other} is not supported")),
        },
        "indent" => editor.run_text_command(TextCommand::Indent),
        "unindent" => editor.run_text_command(TextCommand::Unindent),
        "join_lines" => editor.run_text_command(TextCommand::JoinLines),
        "upper_case" => editor.run_text_command(TextCommand::ChangeCase(Case::Upper)),
        "lower_case" => editor.run_text_command(TextCommand::ChangeCase(Case::Lower)),
        "title_case" => editor.run_text_command(TextCommand::ChangeCase(Case::Title)),
        "swap_case" => editor.run_text_command(TextCommand::ChangeCase(Case::Swap)),
        "transpose" => editor.run_text_command(TextCommand::Transpose),
        "expand_tabs" => editor.run_text_command(TextCommand::ConvertIndentation { to_spaces: true }),
        "unexpand_tabs" => editor.run_text_command(TextCommand::ConvertIndentation { to_spaces: false }),
        "trim_trailing_white_space" => editor.run_text_command(TextCommand::TrimTrailingWhitespace),
        "sort_lines" => editor.run_text_command(TextCommand::ReorderLines(LineOrder::Sort {
            case_sensitive: command.bool_arg("case_sensitive", false),
        })),
        "permute_lines" => match command.str_arg("operation") {
            "reverse" => editor.run_text_command(TextCommand::ReorderLines(LineOrder::Reverse)),
            "unique" => editor.run_text_command(TextCommand::ReorderLines(LineOrder::Unique)),
            _ => editor.notify("permute_lines: only reverse and unique are supported"),
        },
        "run_macro_file" => run_macro(editor, command.str_arg("file")),
        "insert" => editor.run_text_command(TextCommand::Insert(command.str_arg("characters").to_string())),
        "insert_snippet" => editor.run_text_command(TextCommand::Insert(strip_snippet_fields(command.str_arg("contents")))),
        "goto_line" => editor.go_to_line(command.usize_arg("line", 1).saturating_sub(1)),

        // ----- done by the text widget itself --------------------------------------------------
        "undo" | "soft_undo" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Undo)),
        "redo" | "redo_or_repeat" | "soft_redo" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Redo)),
        "select_all" => view.request(ViewRequest::WidgetEdit(WidgetEdit::SelectAll)),
        "copy" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Copy)),
        "cut" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Cut)),
        "paste" | "paste_and_indent" => {
            if let Some(text) = editor.clipboard_text() {
                view.request(ViewRequest::WidgetEdit(WidgetEdit::Paste(text)));
            }
        }

        // ----- preferences and help ------------------------------------------------------------
        "edit_settings" => editor.edit_settings(command.str_arg("base_file"), command.str_arg("default")),
        "reload_packages" => editor.reload_packages(),
        "show_load_errors" => view.show_load_errors = true,
        "show_about_window" => view.show_about = true,

        other => editor.notify(format!("Command not available: {other} (Python plugin commands are not supported yet)")),
    }

    // Whatever the command did, typing continues in the text unless a panel took the focus.
    if view.quick_panel.is_none() && !view.find_panel.focus_requested {
        view.focus_editor = true;
    }
}

/// Closes the quick panel; a cancelled color scheme preview is reverted.
pub fn close_quick_panel(editor: &mut Editor, view: &mut ViewState) {
    if let Some(panel) = view.quick_panel.take() {
        if let Some(scheme) = panel.color_scheme_before {
            editor.preview_color_scheme(&scheme);
        }
    }
    view.focus_editor = true;
}

fn find_next(editor: &mut Editor, view: &mut ViewState, forward: bool) {
    if !editor.find_next(forward) {
        // Nothing to search for yet: open the find panel, as Sublime does.
        view.find_panel.visible = true;
        view.find_panel.focus_requested = true;
    }
}

/// Sublime's built-in line macros (`res://Packages/Default/<Name>.sublime-macro`).
fn run_macro(editor: &mut Editor, file: &str) {
    let name = file.rsplit('/').next().unwrap_or("").trim_end_matches(".sublime-macro");
    let command = match name {
        "Delete Line" => TextCommand::DeleteLine,
        "Add Line" => TextCommand::AddLine { before: false },
        "Add Line Before" => TextCommand::AddLine { before: true },
        "Add Line in Braces" => TextCommand::NewlineAutoIndent,
        "Delete to Hard EOL" => TextCommand::DeleteToLineEdge { to_end: true },
        "Delete to Hard BOL" => TextCommand::DeleteToLineEdge { to_end: false },
        _ => {
            editor.notify(format!("Macro not supported: {file}"));
            return;
        }
    };
    editor.run_text_command(command);
}

/// `${1:default}` becomes `default`; `$1` and `$0` are removed.
fn strip_snippet_fields(snippet: &str) -> String {
    let mut out = String::new();
    let mut chars = snippet.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some(d) if d.is_ascii_digit() => {
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
            }
            Some('{') => {
                chars.next();
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                if chars.peek() == Some(&':') {
                    chars.next();
                }
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}
