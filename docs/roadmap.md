# Roadmap: what to do next (2026-11-26)

Plugins and Package Control are done (`docs/plugins.md`: phases A, B and C). This is the order the rest of the
pending work should go in, most visible in daily use first. The full list of missing commands and settings is
`docs/sublime-gap-analysis.md`; check `docs/sublime-native-commands.md` for a command's arguments before writing it,
and ask Sublime itself where it documents nothing (`docs/plugins.md`, "Checking against Sublime itself").

## 1. Multiple cursors (done, 2026-11-24)

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
  Since 2026-11-25 the gestures come from the mousemaps, not a table: ours
  (`assets/Default/Default (Windows|Linux|OSX).sublime-mousemap`, Sublime's without what we lack), then
  every package's `Default.sublime-mousemap` / `Default (<platform>).sublime-mousemap`, `User` last
  (`entities::mousemap`, `mouse_controller::press_action`). A `drag_select` press makes the selections unless the
  widget does (replacing by characters / words / lines, `extend`); other `press_command`s run at the press and a
  `command` when the button goes up, after the widget's click (Ctrl + click → `goto_definition` works); the side
  buttons switch tabs. `context_menu` stays the views' own.
  Since 2026-11-25 column drags go by the rows drawn (`entities::drag_select::Rows`, from the view's
  `ViewGeometry`): a wrapped line gives a selection per row, columns count from the row's start, and past a wrapped
  row's end the pointer stays on it (`DragPoint::after_row`, `screen_point_after_row`). GUI script:
  `multiple_cursors_over_wrapped_lines_as_drawn`.
  Left for later: the scroll wheel's bindings (`scroll_up` / `scroll_down`: Ctrl + wheel font size), chords
  (another button held), `context`, the scroll bar's mousemaps, `paste_selection_clipboard` (Linux middle click),
  bindings outside a view's text; checking a zero-width column drag over an empty line, and column drags over
  wrapped lines, against Sublime (they cannot be driven by a plugin here; try with `ydotool` on Wayland).
- [x] **Phase 3 — Moving every caret** (2026-11-23): with extra selections, caret keys go to `move` / `move_to`
  instead of the widget: bound ones in `keyboard_controller::bound_command`, unbound ones (Up / Down, Page Up / Down
  and Shift + them) through `command_controller::moves_every_selection` (`app.rs::dispatch_keys`).
  `move {by: lines | pages}` is `text_commands::move_vertically`: a screen column (tabs to the next stop) each
  selection keeps across shorter lines, `Document::goal_column` (Sublime's `xpos`, forgotten at any other
  selection change); carets that meet merge. GUI script: `multiple_cursors_as_drawn` (m07–m09).
  Since 2026-11-25 they move by the rows drawn, the goal column counted from the row's start, as Sublime 4215 does
  (a portable copy with a probe plugin: `word_wrap`, `wrap_width` 20; its answers are in
  `moving_vertically_goes_by_the_rows_of_wrapped_lines_as_sublime_does`). Two differences, both from how the view
  draws: past a wrapped row's end Sublime puts the caret where the next row begins (drawn at the row's end, the row
  remembered), egui would draw it at the next row's start, so ours stays before the space the row broke after; and
  Sublime indents a wrapped line's rows (`indent_subsequent_lines`), the view does not. GUI script:
  `multiple_cursors_over_wrapped_lines_as_drawn`.
- [x] **Phase 4 — Clipboard** (2026-11-23): with extra selections, `copy` / `cut` / `paste` / `paste_from_history`
  are the use cases (`Editor::copy_selections`, `paste_at_selections`, `copy_whole_line`, `paste_whole_line`)
  through `ClipboardGateway`, not the widget (`command_controller`, and `keyboard_controller::clipboard_shortcut`
  for Ctrl+V); the rules are `entities::clipboard`. Copy joins the selected texts with newlines (carets left out);
  with only carets and `copy_with_empty_selection`, each caret's line once, and their paste goes above each caret's
  line (a line each when the counts match); `paste` puts one line in each selection when the clipboard has as many
  lines as there are selections, else the whole text in each. One undo step each. With a single selection the
  widget still does them (its IME, its replays). GUI script: `multiple_cursors_as_drawn` (m10, m11).
  Left for later: Sublime may remember how many selections were copied instead of counting lines; check it.
- [x] **Phase 5 — Keys** (2026-11-24): `find_under_expand_skip` (Ctrl+K Ctrl+D, Cmd+K Cmd+D;
  `Editor::skip_to_next_occurrence`, Find > Quick Skip Next / Quick Add Next). Checked against Sublime 4215 (a
  plugin in a portable copy running both commands on scratch views; the answers are the tables in
  `use_cases/tests/find_replace.rs` and `entities/tests/text.rs`), which also corrected Ctrl+D:
  - Carets first select their word: Sublime's `view.word` (`text::view_word_at`: the word, else the runs of
    blanks / punctuation on each side and a single line break; nothing on an empty line). Ctrl+D then adds nothing.
  - Then both work on the **last selection by position** (not the newest) and look for its text after it,
    wrapping around. Ctrl+D adds the occurrence; Ctrl+K Ctrl+D replaces the last selection with it, or, when every
    occurrence is selected, only takes it away (unless it is the only selection).
  - Whole words only after Ctrl+D started from carets (`EditorState::whole_word_occurrences`, each view its own,
    with `\b`, so `+=` from a caret finds nothing). A selection made by hand matches inside words.
    `find_all_under` (Alt+F3): whole words only from a caret.
  - Since 2026-11-25 Sublime's rule for when they end (two probe plugins in a portable copy, 40 commands): while
    the last command in the view's history (`command_history(0)`) is `find_under_expand` or
    `find_under_expand_skip`. It records a command only when it changes the view's text, selections or regions
    (`move`, `select_lines`, `set_mark`, `single_selection` with several selections, a plugin's command setting
    the selection), and `copy` and `insert` (even empty) always; commands that change nothing (`scroll_lines`,
    `single_selection` with one, `toggle_setting`, `fold`, `clear_fields`, an empty or unknown command), window and
    application commands (`show_overlay`, `toggle_side_bar`…) and another view's commands keep them; so does a
    plugin setting the selection outside a command. Ours: `command_controller::run_rewritten` and
    `Editor::plugin_run_command` compare the views before and after (`Editor::occurrence_views`,
    `end_changed_occurrences`); `copy` / `insert` end them in `note_command_for_occurrences`; the widget's own edits,
    caret keys and clicks (`note_text_edited`, `sync_selection_from_view` when it moved, the multi-selection edits,
    `drag_select`) end them. Tests: `ctrl_d_whole_words_*` in `interface_adapters/tests/command_controller.rs`.
  `unselect_others`, `unselect_current`, `unselect_to_left/right` and `select_to_left/right` were listed here, but
  in Sublime's `Default` they are tab commands (Selection > Tab Selection, Ctrl+J chords): see "Multi-selected tabs"
  below.

## 1b. Multi-selected tabs

Sublime 4 selects several tabs (Ctrl / Shift + click on a tab) and shows them side by side. Commands: `unselect_others`
(Ctrl+J Ctrl+Up), `unselect_to_left/right` (Ctrl+J Ctrl+Left/Right), `unselect_current`, `select_to_left/right`
(Ctrl+J Ctrl+Shift+Left/Right), `focus_to_left/right` (Ctrl+J Ctrl+Page Up/Down), `focus_by_index`; and
`close_selected` / `close_unselected` then mean the selected tabs (today, the tab clicked).

## 2. Find panel toggles (done, 2026-11-26)

`toggle_regex`, `toggle_case_sensitive`, `toggle_whole_word`, `toggle_preserve_case` (Alt+R / C / W / A, on macOS
Cmd+Alt+…, with Sublime's context: `panel_type` `find` and `panel_has_focus`, now false while the text has the
focus) are `Editor::toggle_find_option` (`entities::find::FindToggle`); they work with the panel hidden and leave the
focus where it is (`command_controller::FIND_TOGGLES`). The replace and Find in Files panels show a "Preserve case"
button. `cancel_find` was already Find in Files' stop. Checked against Sublime 4215 (a probe plugin in a portable
copy: `slurp_*`, the toggles, then `replace_all`):
- Preserve case (`find::preserve_case`, also in Find in Files): a match with no capital letter makes the whole
  replacement lowercase, one made only of capitals uppercase (`ß` → `ẞ`); otherwise each character takes the case of
  the match's character at its position, and what goes past the match's end stays as written. After `\1` expands.
- `slurp_find_string` keeps the other options (it used to turn regex and whole word off) and escapes the text
  with regex on; `slurp_replace_string` (Ctrl+Shift+E, Find > Use Selection for Replace) takes the selection, or
  the word at the caret, as written.
Since 2026-11-26 also the panel's other options, Sublime's `toggle_wrap`, `toggle_in_selection` and `toggle_highlight`
(no key binds them; `show_panel` takes `in_selection`, `preserve_case` and `highlight` too):
- "In selection" turned on keeps the active view's selections as the area searched (`find_replace::FIND_AREA`, a
  hidden region set, so it moves with the text; outlined while the panel shows); find next / previous wrap inside it,
  and Find All, Replace, Replace All, the count and the highlights keep to it. Carets alone keep no area (the whole
  text). Opening the panel over several lines with `auto_find_in_selection` (or it on already) turns it on with them.
- "Highlight matches" off stops painting the matches; the panel still counts them.
- Opening the panel over a single-line selection escapes it with regex on (Sublime 4215 does).
- Replace All replaces empty regex matches too (`^`, `$`, `x*`), as Sublime 4215 does; before, `^` changed nothing.
Left for later: when Sublime takes the area exactly (its find panel follows the selection through UI events a
plugin cannot cause, so the probe could not tell toggle time from panel opening).

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
