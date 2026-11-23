# Roadmap: what to do next (2026-11-23)

Plugins and Package Control are done (`docs/plugins.md`: phases A, B and C). This is the order the rest of the
pending work should go in, most visible in daily use first. The full list of missing commands and settings is
`docs/sublime-gap-analysis.md`; check `docs/sublime-native-commands.md` for a command's arguments before writing it,
and ask Sublime itself where it documents nothing (`docs/plugins.md`, "Checking against Sublime itself").

## 1. Multiple cursors (in progress: phases 1 to 4 done)

The most Sublime-like feature still missing ("Not implemented" in CLAUDE.md). Today `Document` keeps the primary
selection (the only one egui's `TextEdit` knows) and `extra_selections`, painted by `editor_view.rs`; typing,
deleting, Left / Right and Home / End reach every selection through `Document::edit_all`. The gestures to copy
are those of Sublime's mousemaps (`Default (Linux|Windows|OSX).sublime-mousemap`, all `drag_select`):

| Gesture | Linux / Windows | macOS |
|---|---|---|
| Add a caret / selection | Ctrl + click / drag | Cmd + click / drag |
| Take one away | Alt + click / drag | Shift + Cmd + click / drag |
| Add a word / line | Ctrl + double / triple click | Cmd + double / triple click |
| Column | Shift + Alt + drag, Shift + right drag; middle drag (Windows) | Alt + drag, middle drag |
| Add a column | Ctrl + Shift + Alt + drag, Ctrl + Shift + right drag; Ctrl + middle drag (Windows) | Cmd + Alt + drag, Cmd + middle drag |

- [x] **Phase 1 — Entities** (2026-11-21):
  - `selection::add_region` / `merged` / `subtract_region`: Sublime's selection set, checked against Sublime 4215
    (a plugin calling `view.sel().add` / `subtract`). Overlapping selections become one in the newer one's
    direction, selections that only touch stay apart, a caret at or inside a selection goes into it; subtracting
    cuts selections (one piece keeps its direction, two go forward), takes the carets it holds and a caret
    strictly inside a selection splits it.
  - `Document::set_selections` (and so `edit_all`, `replace_range`, `follow_buffer`) merges them.
  - `drag_select::drag_selection` (by characters, words, lines) and `column_selections` (screen columns, tabs to
    the next tab stop; lines ending before the column's left side are left out unless all do, then each gets a
    caret at its end).
- [x] **Phase 2 — The mouse** (2026-11-23): `UiEvent::DragSelect` → `Editor::drag_select`
  (`use_cases/src/editing.rs`) replaces, adds (`add_region`) or subtracts (`subtract_region`) the drag's
  selections from those at the press (`EditorState::drag_base`), the drag's so far replaced on every move; taking
  out the last selection leaves them as they were. `mouse_controller::drag_gesture` is the gestures' table per
  platform; `views/drag_select.rs` counts the clicks (double / triple: words / lines), puts the `TextEdit`'s state
  back after it is shown while a gesture goes on, and turns the pointer into a char plus the columns past its
  line's end (`entities::drag_select::screen_point` adds the tabs). GUI script: `multiple_cursors_as_drawn`.
  Left for later: reading the packages' `.sublime-mousemap` instead of the table; columns on wrapped lines count
  from the line's start, not the row's; checking a zero-width column drag over an empty line against Sublime
  (it cannot be driven by a plugin here; try with `ydotool` on Wayland).
- [x] **Phase 3 — Moving every caret** (2026-11-23): with extra selections, caret keys go to `move` / `move_to`
  instead of the widget: bound ones in `keyboard_controller::bound_command`, unbound ones (Up / Down, Page Up / Down
  and Shift + them) through `command_controller::moves_every_selection` (`app.rs::dispatch_keys`).
  `move {by: lines | pages}` is `text_commands::move_vertically`: a screen column (tabs to the next stop) each
  selection keeps across shorter lines, `Document::goal_column` (Sublime's `xpos`, forgotten at any other
  selection change); carets that meet merge. GUI script: `multiple_cursors_as_drawn` (m07–m09).
  Left for later: goal columns over wrapped lines count from the line's start, not the row's.
- [x] **Phase 4 — Clipboard** (2026-11-23): with extra selections, `copy` / `cut` / `paste` / `paste_from_history`
  are the use cases (`Editor::copy_selections`, `paste_at_selections`, `copy_whole_line`, `paste_whole_line`)
  through `ClipboardGateway`, not the widget (`command_controller`, and `keyboard_controller::clipboard_shortcut`
  for Ctrl+V); the rules are `entities::clipboard`. Copy joins the selected texts with newlines (carets left out);
  with only carets and `copy_with_empty_selection`, each caret's line once, and their paste goes above each caret's
  line (a line each when the counts match); `paste` puts one line in each selection when the clipboard has as many
  lines as there are selections, else the whole text in each. One undo step each. With a single selection the
  widget still does them (its IME, its replays). GUI script: `multiple_cursors_as_drawn` (m10, m11).
  Left for later: Sublime may remember how many selections were copied instead of counting lines; check it.
- [ ] **Phase 5 — Keys**: `find_under_expand_skip` (Ctrl+K Ctrl+D), `unselect_others`, `unselect_current`,
  `select_to_left/right` in both keymaps. Then update "Not implemented" in CLAUDE.md / AGENTS.md.

## 2. Find panel toggles

`toggle_regex`, `toggle_case_sensitive`, `toggle_whole_word`, `toggle_preserve_case` (Alt+R / C / W), `cancel_find`,
`slurp_replace_string`: the panel's buttons exist, the commands do not. Small and used all the time.

## 3. Encodings

`default_encoding`, `fallback_encoding`, `set_encoding`, `reopen {encoding}`, `detect_indentation`: about files
opening and saving right, not only comfort.

## 4. Display settings and the status bar

`draw_white_space`, `match_brackets*`, `match_selection`, `font_face`, `draw_indent_guides`, `caret_style`,
`wrap_width`, `line_padding_*`, and the status bar's `show_*`.

## 5. Small commands

`wrap_lines` (Alt+Q), bookmarks (`toggle_bookmark`, `next_bookmark`…), `expand_selection_to_paragraph`,
`switch_file` (Alt+O), `close_tag`, `toggle_overwrite`, the mark (`set_mark`, `select_to_mark`…).

## 6. Larger features, later

Macro recording (`toggle_record_macro`, `save_macro`), the symbol index (`goto_definition`,
`goto_symbol_in_project`), `mini_diff` and git, spell checking, themes (`select_theme`, `ui_scale`), projects and
workspaces, several windows, highlighting above 4 MB.

## Infrastructure

- Run the network tests of `sublime_packages.rs` (`-- --ignored`, last on 2026-11-04) before the next release.
- CI: the SonarQube job (`ci.yml`, needs `SONAR_TOKEN` / `SONAR_HOST_URL`); merge `main` back into `develop`
  after a release (`to-main.yml`).
- Package Control's known limits (`docs/plugins.md`, "Known limits") are taken up when a package needs them.
