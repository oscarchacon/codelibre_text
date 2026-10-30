//! Runs a Sublime Text command by name. Key bindings, menus and the command palette all end here,
//! so packages written for Sublime can call these commands with Sublime's arguments.
//!
//! To add a command: add an arm below that calls a use case, then expose it in
//! `assets/Default/` (keymap, menu, command palette). Names without an arm go to the Python plugins.

use entities::command::Command;
use entities::document::LineEnding;
use entities::text_commands::{Case, LineOrder};
use use_cases::builds::BuildPlan;
use use_cases::editing::{Expansion, TextCommand};
use use_cases::gateways::PluginCommandKind;
use use_cases::plugin_hooks::CommandTarget;
use use_cases::plugin_panels::PluginPanelRequest;
use use_cases::search::{CandidateTarget, SearchKind};
use use_cases::Editor;

use crate::view_state::{InputPanelState, QuickPanel, ViewRequest, ViewState, WidgetEdit};

/// Every frame: answers what plugins ask outside a command (timers, threads) and tells their
/// listeners about edits; the commands they run come back here.
pub fn poll_plugins(editor: &mut Editor, view: &mut ViewState) {
    editor.poll_plugins(&mut |editor: &mut Editor, command: &Command| run(editor, view, command));
    show_plugin_panels(editor, view);
}

/// Shows the quick / input panels plugins asked for. A plugin's quick panel replaces whatever
/// quick panel is open; its input panel replaces the find panel.
fn show_plugin_panels(editor: &mut Editor, view: &mut ViewState) {
    while let Some(request) = editor.take_plugin_panel_request() {
        // The panel takes the keyboard, even if answering the last one gave it back to the text.
        view.focus_editor = false;
        match request {
            PluginPanelRequest::QuickPanel => {
                let Some(selected) = editor.plugin_quick_panel().map(|p| p.selected) else {
                    continue;
                };
                if view.quick_panel.as_ref().is_some_and(|p| p.kind != SearchKind::PluginItems) {
                    cancel_quick_panel(editor, view);
                }
                let mut panel = QuickPanel::new(SearchKind::PluginItems, String::new());
                panel.selected = selected;
                view.quick_panel = Some(panel);
                // Like Sublime, the plugin hears about the item selected at first.
                highlight_plugin_item(editor, view, selected);
            }
            PluginPanelRequest::InputPanel => {
                if editor.plugin_input_panel().is_none() {
                    continue;
                }
                view.input_panel = Some(InputPanelState { focus_requested: true });
                view.find_panel.visible = false;
                editor.hide_output_panel();
            }
        }
    }
}

/// `show_panel {"panel": "output.<name>"}`: the panel takes the place of the find and input panels.
fn show_output_panel(editor: &mut Editor, view: &mut ViewState, name: &str, toggle: bool) {
    if toggle && editor.shown_output_panel().is_some_and(|p| p.name == name) {
        editor.hide_output_panel();
        return;
    }
    if editor.show_output_panel(name) {
        view.find_panel.visible = false;
        finish_plugin_input(editor, view, false);
    } else {
        let panel = format!("output.{name}");
        editor.notify(editor.tr_args("Panel not supported: {name}", &[("name", &panel)]));
    }
}

/// `build {"select": true}`, or several builds for one file: choosing one runs `build` with it.
fn show_build_picker(editor: &mut Editor, view: &mut ViewState) {
    cancel_quick_panel(editor, view);
    editor.refresh_build_options();
    view.quick_panel = Some(QuickPanel::new(SearchKind::BuildSystems, String::new()));
}

/// The answers to a plugin's panels go back to it; the commands its callbacks run come back here.
pub fn answer_plugin_quick_panel(editor: &mut Editor, view: &mut ViewState, item: Option<usize>) {
    editor.answer_plugin_quick_panel(item, &mut |editor: &mut Editor, command: &Command| run(editor, view, command));
    view.focus_editor = true;
    show_plugin_panels(editor, view);
}

pub fn highlight_plugin_item(editor: &mut Editor, view: &mut ViewState, item: usize) {
    editor.highlight_plugin_quick_panel_item(item, &mut |editor: &mut Editor, command: &Command| run(editor, view, command));
}

pub fn change_plugin_input(editor: &mut Editor, view: &mut ViewState, text: &str) {
    editor.change_plugin_input_panel(text, &mut |editor: &mut Editor, command: &Command| run(editor, view, command));
}

/// Enter (`done`) or Esc in a plugin's input panel.
pub fn finish_plugin_input(editor: &mut Editor, view: &mut ViewState, done: bool) {
    if view.input_panel.take().is_none() {
        return;
    }
    editor.finish_plugin_input_panel(done, &mut |editor: &mut Editor, command: &Command| run(editor, view, command));
    view.focus_editor = true;
    show_plugin_panels(editor, view);
}

/// A command chosen in the palette: a plugin command with an input handler first asks for its
/// arguments, in the plugin's panels, and then runs itself.
pub fn run_from_palette(editor: &mut Editor, view: &mut ViewState, command: Command) {
    if editor.start_plugin_command_input(&command) {
        show_plugin_panels(editor, view);
    } else {
        view.queue_command(command);
    }
}

/// Built-in commands that are window commands in Sublime (the others run on the active view), so
/// plugins hear about them in `on_window_command` instead of `on_text_command`.
const WINDOW_COMMANDS: &[&str] = &[
    "new_file", "prompt_open_file", "prompt_open_folder", "prompt_add_folder", "open_file", "open_dir", "save_all",
    "close", "close_file", "close_all", "close_window", "exit", "reopen_last_file", "close_folder_list",
    "refresh_folder_list", "show_overlay", "hide_overlay", "select_color_scheme", "select_language", "show_panel",
    "hide_panel", "toggle_side_bar", "toggle_status_bar", "toggle_full_screen", "toggle_distraction_free",
    "increase_font_size", "decrease_font_size", "reset_font_size", "next_view", "prev_view", "next_view_in_stack",
    "prev_view_in_stack", "select_by_index", "edit_settings", "reload_packages", "show_load_errors",
    "show_about_window", "open_url", "report_issue", "build", "exec", "cancel_build", "set_build_system", "next_result",
    "prev_result",
];

/// Runs a command. Plugins listening to `on_text_command` / `on_window_command` may replace it
/// first, and hear `on_post_text_command` / `on_post_window_command` after it.
pub fn run(editor: &mut Editor, view: &mut ViewState, command: &Command) {
    let command = match hook_target(editor, &command.name) {
        Some(target) => editor.plugin_rewrite_command(command, target),
        None => command.clone(),
    };
    run_command(editor, view, &command);
    if let Some(target) = hook_target(editor, &command.name) {
        editor.plugin_command_ran(&command, target);
    }
}

/// Which hooks hear about a command: plugin commands by their kind (application commands have
/// none), built-in ones by [`WINDOW_COMMANDS`].
fn hook_target(editor: &Editor, name: &str) -> Option<CommandTarget> {
    match editor.plugin_command_kind(name) {
        Some(PluginCommandKind::Text) => Some(CommandTarget::View),
        Some(PluginCommandKind::Window) => Some(CommandTarget::Window),
        Some(PluginCommandKind::Application) => None,
        None if WINDOW_COMMANDS.contains(&name) => Some(CommandTarget::Window),
        None => Some(CommandTarget::View),
    }
}

fn run_command(editor: &mut Editor, view: &mut ViewState, command: &Command) {
    match command.name.as_str() {
        // ----- files ---------------------------------------------------------------------------
        "new_file" => editor.new_file(),
        "prompt_open_file" => editor.prompt_open_files(Some(command.str_arg("initial_directory"))),
        "prompt_open_folder" | "prompt_add_folder" => {
            if let Some(folder) = editor.prompt_add_folder(Some(command.str_arg("initial_directory"))) {
                view.expanded_folders.insert(folder);
                view.show_side_bar = true;
            }
        }
        "open_file" => editor.open_package_file(command.str_arg("file"), command.bool_arg("encoded_position", false)),
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
                    editor.notify(editor.tr_args("Overlay not supported: {name}", &[("name", &other)]));
                    return;
                }
            };
            cancel_quick_panel(editor, view);
            if kind == SearchKind::Commands {
                editor.refresh_plugin_command_states();
                // `{"overlay": "command_palette", "command": "foo", "args": {…}}` goes straight to `foo`.
                let chosen = command.str_arg("command");
                if !chosen.is_empty() {
                    let args = command.args.get("args").cloned().unwrap_or(serde_json::Value::Null);
                    run_from_palette(editor, view, Command::with_args(chosen, args));
                    return;
                }
            }
            view.quick_panel = Some(QuickPanel::new(kind, command.str_arg("text").to_string()));
        }
        "hide_overlay" => close_quick_panel(editor, view),
        "select_color_scheme" => {
            cancel_quick_panel(editor, view);
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
        "select_language" => {
            cancel_quick_panel(editor, view);
            let current = editor.language_setting();
            let mut panel = QuickPanel::new(SearchKind::Languages, String::new());
            panel.results = editor.search(SearchKind::Languages, "");
            panel.results_query = Some(String::new());
            panel.selected = panel
                .results
                .iter()
                .position(|c| matches!(&c.target, CandidateTarget::Language(code) if *code == current))
                .unwrap_or(0);
            view.quick_panel = Some(panel);
        }
        "show_panel" => {
            let toggle = command.bool_arg("toggle", false);
            if let Some(name) = command.str_arg("panel").strip_prefix("output.") {
                show_output_panel(editor, view, name, toggle);
                return;
            }
            let panel = command.str_arg("panel");
            match panel {
                "find" | "incremental_find" | "find_in_files" | "replace"
                    if toggle && view.find_panel.visible && view.find_panel.replace_mode == (panel == "replace") =>
                {
                    view.find_panel.visible = false;
                }
                "find" | "incremental_find" | "find_in_files" | "replace" => {
                    finish_plugin_input(editor, view, false);
                    editor.hide_output_panel();
                    view.find_panel.visible = true;
                    view.find_panel.replace_mode = panel == "replace";
                    view.find_panel.focus_requested = true;
                    editor.use_selection_as_query_if_single_line();
                }
                "console" if toggle => editor.toggle_console(),
                "console" => editor.show_console(),
                other => editor.notify(editor.tr_args("Panel not supported: {name}", &[("name", &other)])),
            }
        }
        "hide_panel" => {
            view.find_panel.visible = false;
            finish_plugin_input(editor, view, false);
            editor.hide_output_panel();
            view.focus_editor = true;
        }

        // ----- builds --------------------------------------------------------------------------
        "build" if command.bool_arg("select", false) => show_build_picker(editor, view),
        "build" => match editor.plan_build(command.str_arg("file"), command.str_arg("variant")) {
            BuildPlan::Run(target) => run(editor, view, &target),
            BuildPlan::Choose => show_build_picker(editor, view),
            BuildPlan::Nothing => {}
        },
        // A plugin's own `exec` replaces this one, as it does in Sublime.
        "exec" if editor.plugin_command_kind("exec").is_none() => {
            if editor.exec(&command.args) {
                show_output_panel(editor, view, "exec", false);
            }
        }
        "cancel_build" => {
            let cancel = editor.cancel_build_command();
            run(editor, view, &cancel);
        }
        "set_build_system" => editor.set_build_system(command.str_arg("file"), command.str_arg("variant")),
        "next_result" | "prev_result" => {
            if editor.next_result(command.name == "next_result") {
                view.find_panel.visible = false;
                finish_plugin_input(editor, view, false);
            }
        }

        // ----- find ----------------------------------------------------------------------------
        "find_next" | "find_prev" => find_next(editor, view, command.name == "find_next"),
        "find_under" | "find_under_prev" => {
            editor.use_selection_as_query(true);
            find_next(editor, view, command.name == "find_under");
        }
        "slurp_find_string" => editor.use_selection_as_query(false),
        "find_under_expand" => editor.select_word_or_next_occurrence(),
        "find_all_under" => editor.select_all_occurrences(),
        "select_lines" => editor.add_carets_on_next_line(command.bool_arg("forward", true)),
        "split_selection_into_lines" => editor.split_selection_into_lines(),
        "single_selection" => editor.single_selection(),
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
            "line_prev" => editor.run_text_command(TextCommand::ExpandSelection(Expansion::LineUpward)),
            "line_without_eol" | "line_strict" => editor.run_text_command(TextCommand::ExpandSelection(Expansion::LineContent)),
            "brackets" => editor.run_text_command(TextCommand::ExpandSelection(Expansion::Brackets)),
            "indentation" => editor.run_text_command(TextCommand::ExpandSelection(Expansion::Indentation)),
            "smart" => editor.run_text_command(TextCommand::ExpandSelection(Expansion::Smart)),
            other => editor.notify(editor.tr_args("expand_selection to {target} is not supported", &[("target", &other)])),
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
        "trim_trailing_white_space" => editor.run_text_command(TextCommand::TrimTrailingWhitespace {
            only_modified: command.bool_arg("only_modified", false),
            trim_on_caret: command.bool_arg("trim_on_caret", true),
        }),
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
        // With nothing selected, copy and cut take the whole line (`copy_with_empty_selection`).
        "copy" | "cut" if editor.copies_whole_line() => editor.copy_whole_line(command.name == "cut"),
        "copy" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Copy)),
        "cut" => view.request(ViewRequest::WidgetEdit(WidgetEdit::Cut)),
        "paste" | "paste_and_indent" if editor.pastes_whole_line() => editor.paste_whole_line(),
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
        "open_url" => editor.open_url(command.str_arg("url")),
        "report_issue" => editor.report_issue(),

        // ----- Python plugins: they can run other commands, which come back here ---------------
        other => {
            if !editor.run_plugin_command(command, &mut |editor: &mut Editor, command: &Command| run(editor, view, command)) {
                editor.notify(editor.tr_args("Command not available: {name}", &[("name", &other)]));
            }
        }
    }

    show_plugin_panels(editor, view);
    // Whatever the command did, typing continues in the text unless a panel took the focus.
    if view.quick_panel.is_none() && !view.find_panel.focus_requested && view.input_panel.is_none() {
        view.focus_editor = true;
    }
}

/// Closes the quick panel and gives the focus back to the text.
pub fn close_quick_panel(editor: &mut Editor, view: &mut ViewState) {
    cancel_quick_panel(editor, view);
    view.focus_editor = true;
}

/// Takes the quick panel away: a cancelled color scheme preview is reverted, a plugin's list hears -1.
fn cancel_quick_panel(editor: &mut Editor, view: &mut ViewState) {
    if let Some(panel) = view.quick_panel.take() {
        if let Some(scheme) = panel.color_scheme_before {
            editor.preview_color_scheme(&scheme);
        }
        if panel.kind == SearchKind::PluginItems {
            editor.answer_plugin_quick_panel(None, &mut |editor: &mut Editor, command: &Command| run(editor, view, command));
            show_plugin_panels(editor, view);
        }
    }
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
            editor.notify(editor.tr_args("Macro not supported: {file}", &[("file", &file)]));
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
