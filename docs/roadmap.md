# Roadmap: what to do next (2026-11-21)

Plugins and Package Control are done (`docs/plugins.md`: phases A, B and C). This is the order the rest of the
pending work should go in, most visible in daily use first. The full list of missing commands and settings is
`docs/sublime-gap-analysis.md`; check `docs/sublime-native-commands.md` for a command's arguments before writing it,
and ask Sublime itself where it documents nothing (`docs/plugins.md`, "Checking against Sublime itself").

## 1. Multiple cursors (in progress)

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
- [ ] **Phase 2 — The mouse**: a `UiEvent::DragSelect { document, from, to, mode }` → `Editor::drag_select`
  (`use_cases/src/editing.rs`), which replaces, adds (`add_region`) or subtracts (`subtract_region`) the drag's
  selections, the ones of the drag so far replaced on every move. `editor_view.rs` reads the modifiers at the press
  and, for these gestures, keeps the press from the `TextEdit` (else it moves its caret and
  `sync_selection_from_view` drops the extras) and turns the pointer into a char (`galley.cursor_from_pos`) or a
  screen point (x / `em_width`, y / `line_height`, folds and wrapping counted). The modifiers per platform come from
  a table (better: read the mousemaps, which also lets packages bind them). To check against Sublime then: a
  zero-width column drag over an empty line (Sublime cannot be driven by a plugin here; try with `ydotool` on
  Wayland). GUI script: Ctrl+click, column drag, typing at every line, with pictures.
- [ ] **Phase 3 — Moving every caret**: with extra selections, Up / Down, Page Up / Down and their Shift / Ctrl
  variants go to the `move` / `move_to` use cases (they already move every selection, `editing.rs`) instead of the
  widget (`keyboard_controller.rs`: today Up / Down pass through and the widget drops the extras). Each selection
  needs a goal column (Sublime's `xpos`) so that crossing a short line does not lose it; carets that meet merge.
- [ ] **Phase 4 — Clipboard**: with extra selections, `copy` / `cut` join the selections' texts with newlines (whole
  lines with `copy_with_empty_selection`), through `ClipboardGateway`, not the widget; `paste` puts one line in each
  selection when the clipboard has as many lines as there are selections, else the whole text in each.
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
