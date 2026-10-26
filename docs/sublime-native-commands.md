# Sublime Text native commands (from the binary)

Generated on 2026-10-26 from `sublime_text.exe` (Sublime Text 4, sha256 `b3bba60b…88f2`) with Ghidra 12.1.4
(headless, `SublimeCommands.java` script) and REA. Only names are recorded here, no code.

**Method.** Every built-in command class has C++ RTTI (`.?AV<name>_command@@`); Ghidra's RTTI analysis gives each
class a `vftable`. For every function in a command's vtable the script listed the identifier-like string literals
it references. Those are the command's registered name, its **argument names** (and values of enum-like arguments,
e.g. the `by` values of `move`) and the **settings** it reads.

Limits: strings referenced only by helper functions (not directly by a vtable function) are missing, and so are
arguments shared by so many commands that they look like noise (e.g. `forward`). Commands registered without a class
(as name + function through `bound_text_command`, e.g. `find_next`, `join_lines`, `split_selection_into_lines`,
`single_selection`, `close_all`, `save_all`) are not in the table; their names are referenced from the view, window
and find-panel setup functions. Commands in Python (`Packages/Default/*.py`, e.g. `toggle_comment`, `sort_lines`)
are not in the binary at all; see `docs/sublime-gap-analysis.md`.

Classes whose registered name differs from the class: `distraction_free` → `toggle_distraction_free`,
`full_screen` → `toggle_full_screen`, `set_option` → `set_setting`, `toggle_option` → `toggle_setting`,
`duplicate_selections` → `duplicate_line`, `remove_all_folders` → `close_folder_list`, `global_*` → the command
without the prefix (used when no window is open).

## Not implemented in CodeLibre Text (120)

| Command | Class (if different) | Arguments / settings it reads |
|---|---|---|
| `add_where_snippet` |  |  |
| `add_word` |  |  |
| `assign_file_type` |  |  |
| `auto_complete` |  | `api_completions_only` `commit_single` `snippets_only` |
| `auto_complete_open_link` |  |  |
| `build` |  | `choice_build_system` `choice_variant` |
| `cancel_build` |  |  |
| `cancel_find` |  |  |
| `chain` |  |  |
| `choose_font` |  | `font_face` |
| `clear_bookmarks` |  |  |
| `clear_fields` |  |  |
| `clear_recent_files` |  |  |
| `clone_file` |  | `add_to_selection` `insertion_index` `retain_selection` `retain_viewport_position` |
| `close_by_index` |  |  |
| `close_by_indices` |  |  |
| `close_deleted_files` |  |  |
| `close_others_by_index` |  |  |
| `close_selected` |  |  |
| `close_tag` |  | `insert_slash` |
| `close_to_right_by_index` |  |  |
| `close_unmodified` |  |  |
| `close_unmodified_to_right_by_index` |  |  |
| `close_unselected` |  |  |
| `context_menu` |  | `event` |
| `copy_as_html` |  | `copy_with_empty_selection` `enclosing_tags` `font_family` `minihtml` |
| `correct_spelling` |  | `correction` |
| `delete_file` |  | `prompt` |
| `delete_folder` |  | `prompt` |
| `detect_indentation` |  | `is_widget` `show_message` `tab_size` `translate_tabs_to_spaces` |
| `drag_select` |  | `additive` `columns` `event` `extend` `subtractive` |
| `expand_selection_to_paragraph` |  | `markup_aware` |
| `expand_snippet` |  |  |
| `focus_by_index` |  |  |
| `focus_group` |  |  |
| `focus_to_left` |  |  |
| `focus_to_right` |  |  |
| `fold` |  |  |
| `fold_all` |  |  |
| `fold_by_level` |  |  |
| `force_reopen_as` |  |  |
| `global_open_file` |  |  |
| `goto_symbol_in_project` |  | `quick_panel` |
| `hide_auto_complete` |  |  |
| `hide_popup` |  |  |
| `history_move` |  |  |
| `hot_exit` |  |  |
| `ignore_word` |  |  |
| `insert_completion` |  | `keep_prefix` `must_insert` `query_change_id` `trigger` |
| `invert_selection` |  |  |
| `move_to_group` |  | `only_last_focused` |
| `new_window` |  |  |
| `new_window_for_project` |  |  |
| `next_bookmark` |  |  |
| `next_field` |  |  |
| `next_misspelling` |  |  |
| `next_modification` |  |  |
| `open_context_url` |  |  |
| `open_file_from_nav_bar` |  |  |
| `open_project_or_workspace` |  |  |
| `open_recent_file` | `global_open_recent_file_command` |  |
| `open_recent_file` |  |  |
| `open_recent_folder` | `global_open_recent_folder_command` |  |
| `open_recent_project_or_workspace` |  |  |
| `open_recent_workspace` |  |  |
| `open_terminal` |  | `terminal_command` |
| `overwrite` |  |  |
| `paste_selection_clipboard` |  | `event` |
| `prev_bookmark` |  |  |
| `prev_field` |  |  |
| `prev_misspelling` |  |  |
| `prev_modification` |  |  |
| `reindent` |  | `force_indent` `single_line` |
| `remove_folder` |  |  |
| `reopen` |  |  |
| `replace_completion_with_auto_complete` |  |  |
| `replace_completion_with_next_completion` |  | `insert_best_completion` |
| `replace_completion_with_prev_completion` |  | `insert_best_completion` |
| `replace_completion_with_tab` |  |  |
| `resize_window` |  |  |
| `reveal_in_side_bar` |  |  |
| `reveal_link_source` |  |  |
| `revert_hunk` |  | `event` |
| `revert_modification` |  |  |
| `run_inline_macro` |  |  |
| `run_macro` |  |  |
| `save_macro` |  |  |
| `scroll_lines` |  | `amount` |
| `select_all_bookmarks` |  |  |
| `select_bookmark` |  |  |
| `select_last_tab` |  |  |
| `select_to_left` |  |  |
| `select_to_right` |  |  |
| `set_build_system` |  |  |
| `set_encoding` |  |  |
| `set_layout` |  |  |
| `set_user_setting` |  |  |
| `show_attribution` |  |  |
| `show_changelog` |  |  |
| `show_progress_window` |  |  |
| `sublime_merge_blame_file` |  | `event` |
| `sublime_merge_file_history` |  |  |
| `sublime_merge_folder_history` |  |  |
| `sublime_merge_line_history` |  |  |
| `sublime_merge_open_repo` |  |  |
| `toggle_bookmark` |  | `toggle_line` |
| `toggle_inline_diff` |  | `event` `prefer_hide` |
| `toggle_menu` |  |  |
| `toggle_minimap` |  |  |
| `toggle_record_macro` |  |  |
| `toggle_save_all_on_build` |  |  |
| `toggle_show_open_files` |  |  |
| `toggle_tabs` |  |  |
| `unfold` |  |  |
| `unfold_all` |  |  |
| `unselect_current` |  |  |
| `unselect_others` |  |  |
| `unselect_to_left` |  |  |
| `unselect_to_right` |  |  |
| `wrap_lines` |  | `rulers` `wrap_width` |

## Implemented — check the arguments and settings (62)

Supported since 2026-10-26: `expand_selection {to: smart | line_prev | line_without_eol | line_strict | brackets | indentation}`,
`copy` / `cut` / `paste` with `copy_with_empty_selection`, `trim_trailing_white_space {only_modified, trim_on_caret}` (and on save
`"all"` / `"not_on_caret"` + `trim_only_modified_white_space`), `show_panel {toggle}`, `prompt_open_file` / `prompt_open_folder
{initial_directory}`, `open_file {encoded_position}`, `reset_font_size` → `default_font_size`.

Still ignored:
- `expand_selection {to: scope | tag}` — needs scopes from the syntax engine.
- `move` / `move_to` / `left_delete` / `right_delete` / `delete_word` are left to the text widget, so `by: subwords/wordends/stops`,
  `move_to: hardbol/hardeol`, `delete_to_tab_stop`, `move_to_limit_on_up_down` and `sub_word_separators` do not apply.
- `next_view` / `prev_view {extend, update_stack}` — selecting several tabs is not supported.
- `open_file {semi_transient, add_to_selection, force_group…}` — no transient tabs, groups or tab multi-selection.
- `line_strict` behaves like `line_without_eol` (its exact difference in Sublime is not known).

| Command | Class (if different) | Arguments / settings it reads |
|---|---|---|
| `close_folder_list` | `remove_all_folders_command` |  |
| `commit_completion` |  |  |
| `copy` |  | `copy_with_empty_selection` |
| `cut` |  | `copy_with_empty_selection` |
| `decrease_font_size` |  |  |
| `delete_word` |  | `delete_to_tab_stop` `sub_words` |
| `duplicate_line` | `duplicate_selections_command` |  |
| `exit` |  |  |
| `expand_selection` |  | `line_prev` `line_strict` `line_without_eol` `smart` |
| `hide_overlay` |  | `cancel` |
| `hide_panel` |  | `cancel` |
| `increase_font_size` |  |  |
| `insert` |  |  |
| `insert_snippet` |  |  |
| `left_delete` |  | `delete_to_tab_stop` |
| `move` |  | `auto_complete_cycle` `empty_line` `extend` `ignore_auto_complete` `is_widget` `line_begin` `line_boundaries` `line_end` `move_to_limit_on_up_down` `punct_begin` `punct_end` `sub_word_begin` `sub_word_end` `sub_word_separators` `subword_boundaries` `subword_ends` `subwords` `wholelines` `wordboundaries` `wordends` |
| `move_to` | `table_move_to_command` | `extend` `hardbol` `hardeol` |
| `move_to` |  | `extend` `force_outer` `hardbol` `hardeol` |
| `new_file` |  |  |
| `new_file` | `global_new_file_command` |  |
| `next_view` |  | `extend` `update_stack` |
| `next_view_in_stack` |  |  |
| `open_dir` | `global_open_dir_command` |  |
| `open_dir` |  |  |
| `open_file` |  | `add_to_selection` `clear_to_right` `encoded_position` `force_clone` `force_group` `replace_mru` `semi_transient` |
| `open_url` |  | `url` |
| `paste` |  |  |
| `paste_and_indent` |  |  |
| `prev_view` |  | `extend` `update_stack` |
| `prev_view_in_stack` |  |  |
| `prompt_open_file` | `global_prompt_open_file_command` | `initial_directory` |
| `prompt_open_file` |  | `initial_directory` |
| `prompt_open_folder` | `global_prompt_open_folder_command` | `initial_directory` |
| `prompt_save_as` |  |  |
| `redo` |  |  |
| `redo_or_repeat` |  |  |
| `reopen_last_file` |  |  |
| `reset_font_size` |  | `default_font_size` |
| `revert` |  |  |
| `right_delete` |  | `delete_to_tab_stop` |
| `run_macro_file` |  |  |
| `save` |  |  |
| `select_all` |  |  |
| `select_by_index` |  |  |
| `select_lines` |  |  |
| `set_file_type` |  |  |
| `set_line_ending` |  |  |
| `set_setting` | `set_option_command` | `is_widget` |
| `show_about_window` |  |  |
| `show_overlay` |  |  |
| `show_panel` |  | `panel_name` `toggle` |
| `soft_redo` |  |  |
| `soft_undo` |  |  |
| `swap_line_down` |  |  |
| `swap_line_up` |  |  |
| `toggle_distraction_free` | `distraction_free_command` |  |
| `toggle_full_screen` | `full_screen_command` |  |
| `toggle_setting` | `toggle_option_command` | `is_widget` `word_wrap` |
| `toggle_side_bar` |  |  |
| `toggle_status_bar` |  |  |
| `trim_trailing_white_space` |  | `only_modified` `trim_on_caret` |
| `undo` |  |  |

## Internal (UI plumbing, licensing, testing) — nothing to do (29)

| Command | Class (if different) | Arguments / settings it reads |
|---|---|---|
| `api` |  |  |
| `append` |  | `disable_tab_translation` `force` `scroll_to_end` |
| `bound` |  |  |
| `bound_text` |  |  |
| `cancel` | `cpo_cancel_command` |  |
| `cancel` | `filter_window_cancel_command` |  |
| `cancel` | `gao_cancel_command` |  |
| `console_python_version` |  | `console` |
| `context_menu` | `tab_context_menu_command` | `event` |
| `cpo_move` |  |  |
| `cpo_select` |  |  |
| `deliberately_crash_for_testing` |  |  |
| `filter_window_move` |  |  |
| `filter_window_select` |  |  |
| `gao_move` |  |  |
| `gao_select` |  | `clear_to_right` `extend` |
| `purchase_license` |  |  |
| `remove_license` |  |  |
| `repeat` |  |  |
| `revert_hunk` | `phantom_revert_hunk_command` |  |
| `scroll_adjust` |  | `acceleration_area` `acceleration_dead_zone` `event` `max_speed` `min_speed` |
| `scroll_drag` |  | `event` |
| `scroll_step` |  | `event` `follow_mouse` |
| `show_license_window` |  |  |
| `table_move` |  | `extend` `subword_ends` `subwords` |
| `toggle` |  |  |
| `toggle_inline_diff` | `phantom_hide_inline_diff_command` | `prefer_hide` |
| `upgrade_license` |  |  |
| `xbound` |  |  |
