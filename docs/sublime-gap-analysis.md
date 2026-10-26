# Sublime Text gap analysis (2026-10-26)

> **Update (Ghidra):** the binary was decompiled with Ghidra 12.1.4. `docs/sublime-native-commands.md` lists the
> 208 native command classes with the arguments and settings each one reads, and which ones CodeLibre Text has.
> New findings: hidden native commands with no menu/keymap entry in Sublime (`invert_selection`, `set_user_setting`,
> `run_inline_macro`, `resize_window`, `close_by_indices`, spelling `add_word` / `ignore_word` / `correct_spelling`),
> and arguments missing from commands that are already implemented — most of them are now supported
> (`expand_selection {to: smart…}`, `copy_with_empty_selection`, `trim_trailing_white_space {only_modified}`,
> `show_panel {toggle}`…); what is still ignored is listed in that file.

Compared against the installed Sublime Text 4 (`C:\Program Files\Sublime Text`):

- `sublime_text.exe` (PE x64, sha256 `b3bba60b…88f2`) — identifier strings (`snake_case`) extracted from the binary
  to tell built-in (C++) commands from Python ones. No decompilation: REA had no native engine configured
  (Ghidra/IDA/Kuna), so this is string-level evidence only.
- `Packages/Default.sublime-package` — every `"command"` in the Windows keymap/mousemap, menus, command palette
  and macros, plus every `*Command` class in its `.py` files; and the keys of `Preferences.sublime-settings`.
- Project side: names matched in `command_controller.rs` / `WIDGET_COMMANDS`, and setting keys read in `crates/*/src`.

Totals: **88 / 287** commands handled, **17 / 200** settings read.

"Native" = the name appears in `sublime_text.exe` (built into the core). "Python" = defined in `Default.sublime-package`
(port them as Rust use cases: Sublime's `Default` package is not loaded as a plugin; Python plugin support is in
`docs/plugins.md`). Single-word names (`fold`, `build`, `select`…)
cannot be told apart by the string scan and are listed under native.

## Commands, grouped by feature

| Feature | Missing commands | Origin |
|---|---|---|
| Multiple cursors | ~~`select_lines`, `split_selection_into_lines`, `find_all_under`~~ (done), `find_under_expand_skip` (Ctrl+K Ctrl+D), `unselect_others`, `unselect_current`, `unselect_to_left/right`, `select_to_left/right`, `find_all` | native |
| Selection | `expand_selection_to_paragraph`, `expand_selection` with `to: scope/tag` (brackets, indentation, smart: done) | native |
| Code folding | `fold`, `unfold`, `fold_by_level`, `unfold_all`, `fold_tag_attributes`, `fold_unfold` | native / py |
| Bookmarks | `toggle_bookmark`, `next_bookmark`, `prev_bookmark`, `clear_bookmarks`, `select_all_bookmarks`, `select_bookmark` | native |
| Mark & kill ring | `set_mark`, `select_to_mark`, `delete_to_mark`, `swap_with_mark`, `yank`, `add_to_kill_ring` | py |
| Navigation history | `jump_back`, `jump_forward`, `_jump`, `add_jump_record` | native / py |
| Symbols / indexing | `goto_definition`, `goto_reference`, `goto_symbol_in_project` (Ctrl+Shift+R) | native |
| Split layouts | `set_layout`, `focus_group`, `move_to_group`, `focus_neighboring_group`, `move_to_neighboring_group`, `new_pane`, `close_pane`, `clone_file` | native / py |
| Tabs | `close_by_index`, `close_others_by_index`, `close_to_right_by_index`, `close_unmodified*`, `close_deleted_files`, `focus_by_index`, `select_last_tab`, `toggle_tabs`, `toggle_show_open_files` | native |
| Find panel | `toggle_regex`, `toggle_case_sensitive`, `toggle_whole_word`, `toggle_preserve_case`, `slurp_replace_string`, `next_result`, `prev_result`, `cancel_find` | native |
| Find in Files | `show_panel` `find_in_files` (UI exists but no search), `add_directory`, `clear_location`, `add_where_snippet`, `find_in_folder` | native / py |
| Auto-complete & snippets | `auto_complete`, `hide_auto_complete`, `expand_snippet`, `next_field`, `prev_field`, `clear_fields` (real snippet fields, not stripped) | native |
| Text editing | `wrap_lines` (Alt+Q), `reindent`, `close_tag`, `auto_indent_tag`, `wrap_block`, `toggle_overwrite`, `scroll_lines`, `show_at_center`, `paste_from_history`, `sort_selection`, `permute_selection`, `convert_ident_case`, `arithmetic`, `rot13`, `encode_html_entities` | native / py |
| Macros | `toggle_record_macro`, `run_macro`, `save_macro` (only `run_macro_file` exists) | native |
| Git / modifications | `next_modification`, `prev_modification`, `revert_modification`, `revert_hunk`, `toggle_inline_diff` (needs `mini_diff`) | native |
| Encoding | `set_encoding`, `reopen` with `encoding`, `detect_indentation` | native |
| Spelling | `next_misspelling`, `prev_misspelling` | native |
| Side bar | `new_file_at`, `new_folder`, `rename_path`, `delete_file`, `delete_folder`, `remove_folder`, `open_containing_folder`, `reveal_in_side_bar`, `focus_side_bar`, `copy_path`, `rename_file` | native / py |
| Recent / projects | `open_recent_file`, `open_recent_folder`, `clear_recent_files`, `*project_or_workspace*`, `new_window`, `save_workspace_as`, `close_workspace` | native |
| Build | `build`, `cancel_build`, `exec`, `set_build_system`, `toggle_save_all_on_build` | native / py |
| UI toggles | `toggle_minimap`, `toggle_menu`, `select_theme`, `customize_theme`, `customize_color_scheme`, `choose_font`, `show_scope_name` | native / py |
| Misc | `switch_file` (Alt+O), ~~`open_url`~~ (done), `open_in_browser`, `open_context_url`, `open_terminal`, `copy_as_html`, `html_print`, `view_resource`, `edit_syntax_settings`, `convert_syntax`, `chain`, `context_menu` | native / py |
| Out of scope | `sublime_merge_*`, license/update commands, `install_package_control`, `new_plugin`, syntax-test/profiling commands | — |

## Settings not yet read (most user-visible first)

- Editing: `auto_match_enabled`, `auto_close_tags`, `smart_indent`, `detect_indentation`, `shift_tab_unindent`, `copy_with_empty_selection`,
  `word_separators`, `sub_word_separators`, `drag_text`, `trim_automatic_white_space`, `indent_to_bracket`, `use_tab_stops`.
- Display: `font_face`, `font_options`, `draw_white_space`, `draw_indent_guides`, `indent_guide_options`, `match_brackets*`, `match_selection`,
  `match_tags`, `caret_style`, `block_caret`, `line_padding_top/bottom`, `wrap_width`, `relative_line_numbers`, `highlight_line_number`,
  `ruler_style`, `margin`, `draw_centered`, `fold_buttons`, `ui_scale`, `theme`, `light/dark_color_scheme`.
- Files: `default_encoding`, `fallback_encoding`, `reload_file_on_change`, `always_prompt_for_file_reload`, `save_on_focus_lost`,
  `binary_file_patterns`, `close_windows_when_empty`, `open_files_in_new_window`.
- Status bar: `show_encoding`, `show_line_endings`, `show_line_column`, `show_syntax`, `show_indentation`, `show_full_path`.
- Find: `find_selected_text`, `auto_find_in_selection`, `close_find_after_*`, `find_in_files_*`.
- Feature-bound (need the feature first): `auto_complete*`, `spell_check`/`dictionary`, `mini_diff`/`git_diff_target`, `show_git_status`,
  `index_*`, `show_definitions`, `hot_exit_projects`, `remember_layout`, `vintage_*`.

## Suggested order

1. Multiple cursors (`select_lines`, `split_selection_into_lines`, `find_all_under`) — builds on `Document::edit_all`.
2. Find panel toggles + `find_all` + Find in Files.
3. `wrap_lines`, `reindent`, `expand_selection_to_paragraph`, `toggle_bookmark`/`next_bookmark`, `switch_file` — small entity-level work.
4. Status-bar and display settings (`draw_white_space`, `match_brackets`, `font_face`, `show_encoding`…).
5. Folding, auto-complete, split layouts, `mini_diff` — larger view work.
