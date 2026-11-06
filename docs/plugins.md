# Python plugins (Sublime Text API)

Status: **phase A done** (2026-10-26); **phase B in progress** (core done 2026-10-28: request ids, calls
outside commands, real timers, view events; then quick and input panels; 2026-10-29 dialogs, resources,
scratch views, `add_on_change`, output panels, statuses and regions drawn, `view.show`, `on_query_context`, command hooks;
2026-10-30 output panel results, build systems, the input panel's view; 2026-11-02 stand-ins for the rest of the API,
`imp` shim, `install_package_control`, `Installed Packages` watched, scopes and the `selector` context; 2026-11-04 a
package with plugins and libraries installed, upgraded and removed by Package Control; 18 popular packages load,
libraries for the Python we run, Package Control upgrades itself; 2026-11-05 Package Control 3's dependencies, a
removal deferred to the next start, `Default.exec`, `view.style` / `style_for_scope`, popups, phantoms, `on_hover`,
auto-complete and plugins' completions, groups and layouts; 2026-11-06 local channels / repositories, unpacked
packages and `revert_package` tried, event commands on the view that asked, `move` by pages / stops / sub-words and
`move_to brackets`, Tools > Build System, region annotations). 2026-11-06: the checkboxes and "missing" lists were
checked against the code (clones, windows, completions, phantoms, groups and `reload_plugin` are done; scopes only lack
exact score values and whole-file scans); the rest of B and phase C below are the pending work, in order. Also on
2026-11-06: projects (`project_data`, `$project*`, project builds), `create_io_panel` with its `on_input` field,
`window.symbol_locations` and the `lookup_*` functions, `PERSISTENT` regions in the session, `set_overwrite_status`,
`new_html_sheet`, `view.export_to_html`, and output panels' `unlisted` / `word_wrap` / `font_size`; `compat.py` (the
standard library names newer Pythons dropped) with four more shims (`pipes`, `cgi`, `imghdr`, `uu`), and the host's Python:
3.13 inside every installer, native x64 and ARM64, offline (`packaging/python/fetch_python.sh`).
Any agent (Claude Code, opencode, …) continuing this should read this file first, then `CLAUDE.md`.

## How it works

Like Sublime Text, plugins run in a separate process (the *plugin host*), so a crash or an endless loop
cannot take the editor down. The host is a Python script run by an external Python 3 interpreter; the
editor and the host exchange one JSON object per line over the host's stdin/stdout.

```
editor (Rust)                                   plugin host (Python, assets/python/)
command_controller ──run "foo"──▶ use_cases::plugins ──PluginHostGateway──▶ plugin_host.py
                                     ▲ plugin_api.rs ◀── call "view_insert" ── sublime.py → sublime_api.py
                                     └── result ──────────────────────────────▶ (returns to the plugin)
                                  ◀── done ─────────────────────────────────────
```

| Piece | File | Circle |
|---|---|---|
| Gateway trait, messages (`PluginMessage`, `PluginLoad`, `PluginCommandKind`) | `crates/use_cases/src/gateways.rs` | Use Cases |
| Loading, running, console, conversation loop (`converse`), events, `poll_plugins` | `crates/use_cases/src/plugins.rs` | Use Cases |
| Where events fire: `on_new`/`on_load`/`on_activated`/`on_*close` (`documents.rs`), `on_*_save` (`saving.rs`) | `crates/use_cases/src/` | Use Cases |
| Listener methods that answer: `on_query_context` (`press_key`), `on_text_command` / `on_window_command` and `on_post_*_command` | `crates/use_cases/src/plugin_hooks.rs` | Use Cases |
| Which built-ins are window commands for the hooks (`WINDOW_COMMANDS`), hooks around every command (`run`) | `crates/interface_adapters/src/controllers/command_controller.rs` | Interface Adapters |
| `poll_plugins` every frame, after the UI events (`command_controller::poll_plugins`) | `crates/frameworks/src/app.rs` | Frameworks |
| The `sublime_api` functions the editor answers | `crates/use_cases/src/plugin_api.rs` | Use Cases |
| Quick / input panels plugins open, and the answers to their callbacks | `crates/use_cases/src/plugin_panels.rs` | Use Cases |
| Popups and phantoms (minihtml), their links, `on_hover` | `crates/use_cases/src/plugin_popups.rs`, `entities/src/minihtml.rs`, `presenters/editor.rs`, `frameworks/src/views/{editor_view,minihtml_view}.rs` | all |
| Showing them (`show_plugin_panels`), answers from the views (`SearchKind::PluginItems`, `UiEvent::InputPanel*`) | `crates/interface_adapters/src/controllers/{command,ui}_controller.rs` | Interface Adapters |
| The input panel view | `crates/frameworks/src/views/input_panel.rs` | Frameworks |
| Output panels (documents that are not tabs, `any_document`), the `append` command, results (`next_result`) | `crates/use_cases/src/output_panels.rs` | Use Cases |
| Build systems: `.sublime-build` parsing, `$file` expansion, selectors, output results | `crates/entities/src/build.rs` | Entities |
| `build`, `exec`, `cancel_build`, `set_build_system`, `poll_builds` | `crates/use_cases/src/builds.rs` | Use Cases |
| The process of a build (`ProcessGateway`) | `crates/frameworks/src/gateways/child_processes.rs` | Frameworks |
| `show_panel output.<name>` (`show_output_panel`), its presenter and view | `command_controller.rs`, `presenters/output_panel.rs`, `frameworks/src/views/output_panel.rs` | Adapters / Frameworks |
| Edits from plugins move every selection and region set (`Document::replace_range`, `RegionSet`, `request_show`) | `crates/entities/src/document.rs` | Entities |
| Regions drawn (fill, outline, underlines, gutter icons), the scope's color (`SyntaxEngine::scope_color`), `view.show` scrolling | `crates/frameworks/src/views/editor_view.rs`, `presenters/editor.rs` (flags → `RegionViewModel`) | Frameworks / Adapters |
| Plugin commands are the fallback of the command controller; `show_panel console` | `crates/interface_adapters/src/controllers/command_controller.rs` | Interface Adapters |
| Arbitrary `.sublime-settings` files (`load_settings_file`, `*_user_settings_file`), `find_resources` / `load_resource` | `crates/interface_adapters/src/gateways/sublime_resources.rs` (names ↔ files: `package_files.rs`) | Interface Adapters |
| Message dialogs plugins show (`DialogGateway::show_message`, `rfd::MessageDialog`) | `crates/frameworks/src/gateways/native_dialogs.rs` | Frameworks |
| Process, interpreter discovery, reader threads (wake the window), call ids, 20 s timeout | `crates/frameworks/src/gateways/python_plugin_host.rs` | Frameworks |
| Host, transport, our own `sublime` / `sublime_plugin` modules | `assets/python/*.py` (embedded with `include_str!`, written to `%APPDATA%\codelibre_text\data\Lib\python` at start) | — |
| Fake host for tests (`PluginScript`, `plugin_call`, `World::with_plugin_commands`) | `crates/test_support/src/lib.rs` | tests |

`sublime.py` and `sublime_plugin.py` are **our own code**, written from the public API reference
(https://www.sublimetext.com/docs/api_reference.html). Do not copy Sublime's files from
`C:\Program Files\Sublime Text\Lib\python3*\` (proprietary license); reading them to learn a function's
behaviour is fine. Their `sublime_api` function names are a good guide for naming new functions.

### Protocol

```
editor → host  {"op":"load","packages":[{"name":"MyPkg","dirs":["…"]}],"dependencies":["…"],"paths":{"packages":…,"installed_packages":…,"cache":…,"executable":…}}
host → editor  {"op":"loaded","commands":[["hello","text"],…],"events":["on_modified","on_load_async",…],"errors":["traceback",…]}
editor → host  {"op":"run","kind":"text|window|application","name":"hello","args":{…},"view":7,"window":1}
editor → host  {"op":"event","name":"on_pre_save","view":7}
editor → host  {"op":"callback","id":3,"name":"select","args":[2]}        (a panel's answer)
editor → host  {"op":"query","name":"on_query_context","view":7,"window":1,"args":["key","equal",true,false]}
host → editor  {"op":"call","id":12,"fn":"view_insert","args":[7,0,"hi"]}   (zero or more; also between requests)
editor → host  {"op":"result","id":12,"value":2}   or   {"op":"result","id":12,"error":"the view is closed"}
host → editor  {"op":"log","text":"printed text"}                         (any time)
host → editor  {"op":"done","error":null}   or   {"op":"done","error":"Traceback …"}
host → editor  {"op":"answer","value":true}   (ends a query: true / false, ["command", {args}], or null)
editor → host  {"op":"exit"}
```

While the host waits for a `result` it may receive a nested `run` (a plugin's `run_command` of another
plugin command); it runs it and answers `done` before going on. The editor does the same in `converse`.
A plugin's `run_command` of a built-in command goes back through the command controller (the use case
receives it as a `CommandRunner` callback, because built-in commands live in an outer circle).

Threads (`codelibre_rpc.py`): a reader thread takes every line from stdin; a `result` goes to the thread
whose `call` has that `id`, any other message is a request for the main thread. So the API works from any
thread. The editor answers calls in nesting order: the gateway keeps a stack of open call ids, and
`reply` answers the innermost. Between requests the host's main loop runs due `set_timeout` callbacks and
waits for the next request with the next timer's delay; `set_timeout_async` callbacks and the `*_async`
listeners run on one "sublime async" thread. Calls made then are answered by `Editor::poll_plugins`,
which `app.rs` calls every frame; the gateway's reader threads wake the window (`with_waker`,
`ctx.request_repaint()`) when the host writes something.

Events: the host reports in `loaded` which events some listener defines, and the editor sends only those
(an `_async` listener makes the editor send the sync name; the host queues the async call). Listener
tracebacks go to the console; the event still answers `done` with no error. Built-in commands that a
listener runs that only edit text run at once through `Editor::set_event_command_runner`; the others are queued for
the next `poll_plugins` (events fire inside use cases, which have no `ViewState`); plugin commands run at once, so a formatter's own `TextCommand` in `on_pre_save`
edits the text before it is written. `on_modified` / `on_selection_modified` are found by
`poll_plugins` comparing each document's `change_count` (bumped by `Document` edits and by
`UiEvent::TextEdited` from the text widget) and the active one's selections with the previous frame.

### Conventions

- Positions are char indices (like `Selection`); view id = `DocumentId`; windows are ids from 1 (`WindowId`, the first
  is 1). Clones share a `Document::buffer`; each view has its own selections, regions, phantoms and scroll.
- `view.sel()` sends the whole list on every change (`view_selection_set`); the **last** region becomes
  the primary selection the text widget shows. A cleared selection stays on the Python side until refilled.
- Settings targets: `["view", id]`, `["file", "Name.sublime-settings"]`, `["window", 1]`.
  `Preferences.sublime-settings` changes apply at once (session overrides) and are written on `save_settings`.
- Plugins load from every package folder with top-level `.py` files (ours and, with `import_sublime_packages`,
  Sublime's, except its `Default`), each package imported as a Python package so relative imports work.
- Settings: `python_plugins` (default true), `python_interpreter` (default: the Python 3.13 bundled with the editor,
  else `py -3.13`, `python3.13`, `py -3`, `python3`, `python`, `uv python find`).
- Output and tracebacks go to the console (`Ctrl+\``, View > Show Console); load errors also go to Help > Package Load Errors.
  Load errors, plugin command failures, a dead host and Package Control install failures are also written, with
  their UTC time, to the error log (`data\Local\Errors.log`, Help > Error Log; `Editor::record_errors`), and
  Help > Report an Issue fills the bug form's Errors field with this session's ones.
- Saving a `.py` file in a package reloads the plugins (`reload_plugins`); Preferences > Reload Packages too. The host
  keeps running: only the packages added, removed or whose `.py` files changed (size or date, subfolders too) are loaded
  again (`load` in `plugin_host.py`); the others keep their modules, listeners and threads.

### Adding an API function (the usual change)

1. `assets/python/sublime.py`: the method, calling `sublime_api.<name>(...)`.
2. `crates/use_cases/src/plugin_api.rs`: a match arm for `<name>` (view functions start with `view_`).
3. A test in `crates/use_cases/tests/plugin_api.rs` (`call(&world, &mut editor, "<name>", json!([...]))`).
4. If the Python side has logic, a test in `crates/frameworks/tests/python_plugin_host.rs` (real Python; skipped without one).

## Phase A — done

- `sublime_plugin`: `ApplicationCommand`, `WindowCommand`, `TextCommand` (`run`, `is_enabled`; name derived like Sublime,
  e.g. `HTMLPrintCommand` → `h_tMLPrint`), `plugin_loaded` / `plugin_unloaded`; the other base classes exist but are not called.
- `sublime`: `Region`, `Selection`, `View` (text, insert/erase/replace, sel, line/full_line/lines/split_by_newlines/word,
  rowcol/text_point, find/find_all, settings, run_command, name/file_name/dirty/read-only/encoding/line endings/syntax),
  `Window` (active_view, views, new_file, open_file incl. `ENCODED_POSITION`, find_open_file, focus_view, folders,
  project_data, extract_variables, run_command, settings), `Settings`, `load_settings`/`save_settings`, `run_command`,
  `status_message`/`error_message`/`message_dialog`, clipboard, version/platform/arch/paths, constants.
- Stand-ins (so plugins do not crash, but nothing visible happens yet), for what phase C still has to finish:
  `show_popup_menu` (a quick panel), folding, the view's geometry (`text_to_layout`…, a fixed 8×16 font), sheets
  (`TextSheet` / `ImageSheet`). (Meta and symbol information, completions, groups / layouts, `list_syntaxes` /
  `find_syntax_*`, scopes, styles, popups and phantoms were stand-ins too; phase C made them real. `HtmlSheet` and
  `new_html_sheet` are real: a read-only tab with the plain text of the minihtml.)
  (`set_timeout*`, the dialogs, `is_scratch`, `add_regions`, `set_status` and `show` were stand-ins too; phase B made them real.)

## Phase B — events, panels, async (in progress)

Goal: plugins that react to the editor and show UI (linters, formatters on save, quick-panel pickers).

- [x] **Request ids in the protocol** (`"id"` on `call`/`result`): the API works from any Python thread.
- [x] **Calls outside a command**: `PluginHostGateway::try_receive`; `Editor::poll_plugins` every frame (woken by the host).
- [x] **Real timers**: `set_timeout(cb, delay)` on the main loop, `set_timeout_async` on the async thread (`sublime._Timers`).
- [x] **Event listeners** `EventListener` and `ViewEventListener` (`is_applicable`, one object per view, dropped at
      `on_close`): `on_new`, `on_load`, `on_activated`/`on_deactivated`, `on_pre_save`/`on_post_save`,
      `on_pre_close`/`on_close`, `on_modified`/`on_selection_modified`, and their `*_async` variants. `view.change_count()`.
- [x] Other events: `on_load` also for the files the session restores, `on_revert`, `on_reload` (a file changed on
      disk), `on_init(views)` (after `Editor::start`, and after a reload for the listeners it loaded), `on_exit()` (the window
      closes, `Editor::plugin_exit`).
- [x] **`TextChangeListener`** (`on_text_changed`, `_async`, `on_revert`, `on_reload`): `Document::log_text_changes`
      keeps a copy of the text and compares it after each edit (the widget edits in place), once a frame the
      changes go in an `on_text_changed` query. Several carets count as one change from the first to the last.
      The columns of `HistoricPosition` are chars, also in `col_utf8` / `col_utf16`.
- [x] Clones and windows: `on_clone` (also for `ViewEventListener`s, with `applies_to_primary_view_only`) and
      `on_new_window` / `on_pre_close_window`; `view.clones()` / `view.is_primary()`. (`on_hover` is done, with popups:
      phase C.)
- [x] Built-in commands that edit text (`EDITING_COMMANDS` in `command_controller.rs`) run at once inside an event or
      query, on the view that asked (`Editor::set_event_command_runner`, set by `compose()`), through the hooks, so a
      formatter's `view.run_command(...)` in `on_pre_save` reaches the file.
- [x] Built-in commands that change only selections, the find string or settings (`EDITOR_COMMANDS` in
      `command_controller.rs`: `split_selection_into_lines`, `find_all_under`, `set_setting`, `set_file_type`…) also run at once.
      The others (panels, the widget's undo / clipboard, `save`, `close`…) wait for the next frame, and then run on the view
      that asked (`queue_plugin_command` keeps it; the active view comes back afterwards).
- [ ] Still not at once: `save` / `close`, `select_all`, the widget's undo / clipboard and the panel / build commands;
      they change views or `ViewState` under the use case that fired the event, so they wait for the next frame
      (`runs_during_event` in `command_controller.rs`).
- [x] **`on_query_context`**: a key binding condition the keyboard controller's context does not know goes to the
      listeners (`Editor::press_key` wraps the context; the keymap is set aside while they answer). The operator
      becomes `sublime.OP_*`; the first `True` / `False` wins, `None` from everyone means the binding does not apply.
      `match_all` goes to the listeners.
- [x] **`on_text_command` / `on_window_command`**: `command_controller::run` asks before every command; a `(name, args)`
      answer runs instead (once: the new command is not asked about again). `on_post_text_command` /
      `on_post_window_command` after it, with the command that ran. Plugin commands go by their kind (application
      commands have no hook); built-ins by `WINDOW_COMMANDS`, the rest are text commands (only with an open view).
      Built-in commands a listener runs while answering are handled as in events.
- [x] Hooks around the widget: while a listener of text commands exists and the text has the focus, the keys of undo,
      redo, select all, copy, cut and paste run as commands (`hooks_widget_edits` in `keyboard_controller.rs`), so a
      listener can replace them; `on_post_text_command` waits for the frame in which the widget did the edit
      (`ViewState::widget_edits_in_progress`). Without listeners the keys go straight to the widget, as always.
- [x] Hooks around typing, deleting and caret movement: with the text focused and a listener of text commands, the keys
      the widget would take itself become commands — `insert` for typed characters, Enter and Tab; `left_delete` /
      `right_delete` for Backspace / Delete; `move` / `move_to` for the arrows, Home / End and Page Up / Page Down
      (`widget_key_input` in `app.rs`, `hooked_widget_input` in `command_controller.rs`). A listener may replace them
      (`insert` with other characters, or any other command, which runs at once). When it does not, the key or the
      characters go back to the widget (`WidgetEdit::Insert` / `WidgetEdit::Key`, applied before it draws), so undo, IME
      and the completion list stay the widget's; `on_post_text_command` fires after that frame. With several selections the
      widget only knows the primary one, so the use cases do the edit instead (as without listeners). Without listeners
      nothing changes: every key goes straight to the widget.
- [x] `on_post_text_command_async` / `on_post_window_command_async` run on the async thread; a listener that defines only
      the `_async` form still makes the editor ask (`sublime_plugin.events` advertises the sync name).
- [ ] Hooks, still missing: a `left_delete` / `move` the user binds in a keymap runs as a command (the use case, not the
      widget), so it is outside the widget's undo.
- [x] **Quick panel**: `window.show_quick_panel(items, on_select, flags, selected_index, on_highlight, placeholder)`;
      items are strings, lists (caption, detail lines) or `QuickPanelItem`s (detail lines joined with ` · `; the minihtml of details and annotations is shown as text,
      annotation as the hint, `kind` ignored); `MONOSPACE_FONT`, `WANT_EVENT` (the event is `{"modifier_keys": {}}`).
      Python keeps the callbacks by id (`sublime._callbacks`); the editor answers `select` (index in the plugin's
      list, -1 when dismissed or replaced by another quick panel) and `highlight` (also for the first selection).
- [x] **Input panel**: `window.show_input_panel(caption, initial, on_done, on_change, on_cancel)`, a one-line panel at
      the bottom in place of the find panel; Esc (`hide_panel`) or opening Find cancels it. Answers `done`, `change`,
      `cancel`. It returns a real view: a `Document` in `PluginInputPanel` (found by `any_document`, gone with the
      panel), so `sel()`, `substr`, `insert`, `settings` and plugin `TextCommand`s work on it; the view pushes edits
      (`InputPanelChanged`) and the caret (`InputPanelSelected`) and takes selections by `selection_revision`.
      `window.active_panel()` answers `input`. `on_change` is not called for a plugin's own edits.
- [x] `KEEP_OPEN_ON_FOCUS_LOST`, the `kind` letter in a badge, annotations right-aligned, `sublime.ListInputItem`.
      The badge has one color: the kinds' own colors and custom kinds are not drawn.
- [x] **Output panels**: `window.create_output_panel(name)` (also `get_output_panel`) returns a view: a document kept
      apart from the tabs (`EditorState::output_panels`; view functions find it through `any_document`), never dirty,
      emptied when created again. `show_panel {"panel": "output.<name>", "toggle"}` shows it at the bottom in place of
      the find / input panels; `hide_panel` (Esc: `panel_visible` is true, `panel_has_focus` is not) hides it.
      `find_output_panel`, `destroy_output_panel`, `window.active_panel()` (only output panels), `window.panels()`.
      Sublime's `append` command (`characters`, `force` for read-only panels) runs in the use case, for panels and tabs;
      plugin `TextCommand`s run with the panel as their view; other built-in commands refuse to run in a panel.
      The view draws the last 2000 lines, following the end.
- [x] **Output panel results**: the view settings `result_file_regex` / `result_line_regex` / `result_base_dir` (Rust's
      `regex`: no lookaround). `next_result` / `prev_result` (F4, Shift+F4) go through the last panel shown that has them
      and wrap around; double-clicking a result opens it; the one visited is highlighted and scrolled to. Its message
      goes to the status bar. `view.find_all_results()`, `find_all_results_with_text()`.
- [x] **Motions as commands**: `move` (`by`: `characters`, `lines`, `pages` (the lines the view shows, less one), `words`,
      `word_ends`, `subwords` / `subword_ends` (camelCase, `snake_case` and punctuation), `stops` with `word_begin`,
      `word_end`, `punct_begin`, `punct_end`, `sub_word_begin`, `sub_word_end`, `line_begin`, `line_end`, `empty_line`;
      `forward`, `extend`) and `move_to` (`bol`, `eol`, `hardbol`, `hardeol`, `bof`, `eof`, `brackets`, `extend`), every
      selection, in tabs, panels and events (`entities::text_commands::{move_by, move_to}`). Key bindings to what the widget
      does itself (characters, lines, words, pages, line and text ends) go to it; the others run as commands
      (`keyboard_controller::widget_does`): Ctrl+M (`move_to brackets`), Alt+Left / Alt+Right (sub-words; Ctrl on macOS).
      Missing: `bol` / `eol` of wrapped lines (they go to the logical line's), `separators` / `sub_word_separators`,
      `move_to_limit_on_up_down`.
- [x] **Build systems**: every `*.sublime-build` in the packages, with the platform's overrides and `variants`. `build`
      (Ctrl+B, F7) uses the build set by `set_build_system` or picked before, else the one that applies by `selector` (the
      syntax's base scope), `file_patterns` or `keyfiles` (next to the file or above it, or in an open folder); several
      open the picker, as does `build {"select": true}` (Ctrl+Shift+B). `variant`, `target`, `cancel` work. Arguments expand
      `$file`, `$file_path`, `$file_name`, `$file_base_name`, `$file_extension`, `$folder`, `$packages`, `$platform`, also
      `${x:default}` and `${x/regex/replacement/}` (not in `file_regex` / `line_regex`); unknown names stay, for the shell.
      `sublime.expand_variables`. Settings: `save_all_on_build`, `show_panel_on_build`.
- [x] **`exec`** (a built-in here, since we skip `Default`): `shell_cmd`, `cmd` (`shell`), `working_dir` (default the
      file's folder), `env`, `path`, `file_regex`, `line_regex`, `quiet`, `kill`. One process at a time (`ProcessGateway`),
      read by threads that wake the window; a new build replaces the running one; `cancel_build` kills the whole tree. A
      plugin's own `exec` command wins. Ignored: `encoding` (UTF-8, lossy), `syntax`, `word_wrap`.
- [x] **Tools > Build System**: Automatic, then every build system by name (`{"command": "$build_systems"}` in the menu,
      expanded by `presenters::menu`, read at each package load: `Editor::menu_build_systems`); each runs
      `set_build_system`, the one set is checked. Builds inside zipped packages load since `.sublime-package` support.
- [x] **Projects**: a `.sublime-project` opened from File > Open Project…, the palette (`project_open`) or a plugin is a
      project (`entities/src/project.rs`): its `folders` become the window's folders (`set_folders`,
      `sync_project_folders`), its `settings` layer over the global ones, its `build_systems` are builds (with
      `$project`, `$project_path`, `$project_name`, `$project_base_name`, `$project_extension`, `$project_file`,
      `$project_file_path`) and `project_data` / `set_project_data` read and write it (the file is saved), with
      `extract_variables` understanding `$project*`. The commands are `prompt_open_project` / `open_project` and
      `close_project`, with Sublime's `prompt_open_project_or_workspace` / `open_project_or_workspace` as aliases
      (workspaces are not read). The session keeps the project open.
- [ ] Builds, still missing: variants in the Build System menu. There is no Ctrl+Break: egui has no such key.
- [ ] Output panels, still missing: the panel's own syntax highlighting, selections drawn, built-in commands other than
      `VIEW_COMMANDS` (`editing.rs`: `move`, `move_to`, `left_delete`, `right_delete`, `delete_word`, `select_all`,
      `insert`), the console as a panel (it is a tab), results in Find Results (find in files is not implemented).
      `append` and result lines work in panels; `unlisted`, `word_wrap` and `font_size` of the view are respected.
- [x] **Command input handlers**: a command chosen in the palette (or `show_overlay` with `command`) asks
      `command_input`; when `input()` returns a handler, `sublime_plugin._CommandInput` asks in the existing panels
      (`ListInputHandler` in the quick panel, `TextInputHandler` in the input panel, with `validate`, `confirm`,
      `cancel`, `next_input`, `BackInputHandler`, `sublime.ListInputItem`) and runs the command itself. Key bindings
      and menus run the command directly. `preview` goes to the status bar.
- [ ] Input handlers, still missing: they are not inside the palette as in Sublime (no breadcrumb, `Backspace` does
      not go back to the previous handler), `want_event`, HTML previews. `initial_selection` keeps its regions.
- [x] **Command metadata**: `is_visible`, `is_enabled`, `is_checked`, `description` of every plugin command in the
      menus and the palette, asked in one `command_states` query when a menu or the palette opens (`plugin_commands.rs`)
      and only if some command defines them. Menus hide, grey out and check their items; the palette leaves out
      hidden and disabled commands; `description` is the caption of an entry without one.
- [x] **`Settings.add_on_change`** / `clear_on_change`: kept by settings object in `sublime._on_change`, called after
      a plugin's own `set` / `erase` (through any `Settings` of the same file).
- [x] `add_on_change` for the editor's own changes: the callback's settings object is registered (`settings_watch`)
      and `poll_plugins` compares its values every frame, so any change counts (`set_setting`, the user saving a
      settings file…); a `settings_changed` query calls the callbacks. A plugin's own `set` does not come back.
- [x] **Dialogs**: `message_dialog` / `error_message` (also logged to the console), `ok_cancel_dialog`,
      `yes_no_cancel_dialog` (`DialogGateway::show_message`, custom labels, the plugin's title or "CodeLibre Text"),
      `open_dialog`, `save_dialog` (`name.extension`), `select_folder_dialog`. File dialogs answer through a
      `set_timeout` after the call, as in Sublime. `file_types` become the dialog's filters and `multi_select`
      works in `select_folder_dialog`. `allow_folders` is ignored: the native dialog picks either files or folders.
- [x] **Statuses**: `view.set_status/get_status/erase_status`, kept by the editor per view
      (`PluginState::view_statuses`, dropped with the view) and shown after the caret position, in key order.
      `view.set_overwrite_status` / `overwrite_status` (`PluginState::overwrite_views`, `OVR` in the status bar).
- [x] **Regions drawn**: `view.add_regions(key, regions, scope, icon, flags)` / `get_regions` / `erase_regions` keep a
      `RegionSet` in the `Document`, moved by its edits (`replace_range`). Drawn under the text in the scope's color
      (the color scheme's rule, or a built-in color for `region.<color>ish`): filled and outlined unless
      `DRAW_NO_FILL` / `DRAW_NO_OUTLINE`, solid / stippled / squiggly underlines, `DRAW_EMPTY` as a bar,
      `DRAW_EMPTY_AS_OVERWRITE` as a block, `HIDDEN` not drawn; `dot` / `circle` / `bookmark` icons in the gutter
      (any other icon is a dot). `PERSISTENT` sets go to the session (`SessionDocument::regions`) and come back with
      the file.
- [x] Regions move with every edit, also the widget's: `add_regions` keeps a copy of the text, compared in
      `Document::note_text_changed`. An edit with several selections is one change from the first caret to the
      last, so regions between the carets collapse.
- [x] **Annotations**: `add_regions(annotations=[minihtml…], annotation_color, on_navigate)` — each region's annotation at
      the right edge of the view on its first line (several on a row stack leftwards), with the region's color (or
      `annotation_color`) as frame (`RegionSet::annotations`, `editor_view.rs::show_annotations`); a link calls
      `on_navigate(href)` (`PluginState::region_callbacks`, `UiEvent::AnnotationLinkClicked`). Drawn over the text of long
      lines; `on_close` is never called (they cannot be closed). The link click was checked in tests, not yet by hand.
- [ ] Regions, still missing: icon files, the minimap.
- [x] **Resources**: `sublime.find_resources(pattern)` (file name with `*` / `?`, as `Packages/<pkg>/<path>`, each
      once, ours over Sublime's), `load_resource` (UTF-8, `\n` line endings), `load_binary_resource` (sent as hex);
      a missing one raises `FileNotFoundError`. `decode_value` / `encode_value`. Zipped packages are unpacked, above.
- [x] `view.change_count()`, `view.is_scratch/set_scratch` (`Document::scratch`: never dirty), `view.close(on_close)`
      (a dirty view asks first and answers false).
- [x] `view.show/show_at_center` (point, region or selection): `Document::request_show`, followed by the view
      like `selection_revision`. `visible_region` is what the view last drew; `viewport_*` still answer the whole text.

## Phase C — Package Control and full compatibility

Goal: install and run Package Control and the popular packages (LSP, GitGutter, …).

- [x] **`.sublime-package` files**: unpacked by `gateways/package_archives.rs` at every `load_packages` (only those that
      changed: a stamp file holds size and date) to `data/Cache/Packages/{shipped,sublime,own}`, which then are folders of
      packages like any other (`PackagePaths::roots`, load order: those, Sublime's loose packages, ours). So resources,
      `load_resource` and the plugin host (no `zipimport`) work on plain files. Plugins still see the paths inside the
      archive, `<Installed Packages>/<package>.sublime-package/<file>` (`ArchiveFinder` / `ArchiveLoader` in `plugin_host.py`
      read from the cache): Package Control works out its folders from `__file__`, so they can only be read with
      `load_resource`, as in Sublime. A file of
      a later place overrides the same file of an earlier one (`find_resources`, `load_package`). Sources: Sublime's
      install `Packages`, Sublime's `Installed Packages`, our `Installed Packages`. Sublime's `Default` and `Package
      Control` are not imported (the first targets its core; the second uses the network at load: it runs only from our
      own `Installed Packages`). Names that climb out of the package are not written.
- [x] Of what Sublime ships, syntaxes and `tests/` are left out (`ArchiveFolder::shipped`): syntect cannot load most of
      them (`extends`, `branch_point`: 124 errors) and has its own for those languages. Packages the user installed
      do load their syntaxes. The first start unpacks them (about 11 s in a debug build, in the window's thread).
- [x] **`ignored_packages`**: such packages are not found (`package_files::ignored_packages`, default `["Vintage"]`, which
      would take every key) and their plugins are not loaded. A change reloads the packages (Package Control's
      disabling, below).
- [x] **Libraries for the Python we run**: `sublime.version()` is the Sublime build whose plugin host runs that Python
      (`4203` for 3.14+, `4201` for 3.13, else `4200`), and Package Control installs every library for it, compiled ones
      too (`pywinpty`, `orjson`), in `Lib/python314` / `Lib/python313` (else `Lib/python38`). `add_library_folders` in
      `plugin_host.py` puts that folder first on `sys.path`, then `python38` and `python33` (libraries installed before).
      Our default `"disable_plugin_host_3.3": true` tells Package Control there is no 3.3 host (else it finds Sublime's
      `plugin_host-3.3` next to `executable_path()`), so it supports one Python only and runs every package on it: 3.3
      libraries are too old for a current Python (pygments 2.1 does not compile). `.python-version` is not read.
      `sublime.arch()` is the processor's (`x64`, `x32`, `arm64`): the build of a compiled library depends on it.
      The version also makes Package Control offer the releases for that build (BracketHighlighter, ColorHelper,
      MarkdownPreview 3…), written for that Python; plugins that test the version may call API it added.
- [x] **`dependencies.json`** needs nothing of ours: Package Control 4 reads it itself (`get_libraries`) and installs those
      names as libraries, like a package's `libraries`.
- [x] **Package Control 3's dependencies** left in a profile it never migrated (`Packages/<dep>` with
      `dependency-metadata.json` or `.sublime-dependency`; Sublime's own profile, which we only read): `plugin_packages`
      sends them apart (`PluginLoad::dependencies`), never as plugins, and skips `0_package_control_loader`;
      `set_dependency_folders` in `plugin_host.py` puts one folder of each on `sys.path`, after the libraries, picked as
      Package Control 4 does when it migrates one: `st4_py<ours>_<platform>_<arch>`, `st4_py<ours>_<platform>`,
      `st4_py38`, `st4`, `st3`, `all` (`st3_<platform>*` are compiled for 3.3: left out).
- [x] **Package Control 4 migrates them** in our own profile (`bootstrap._migrate_dependencies`, when
      `0_package_control_loader.sublime-package` is in our `Installed Packages`): each to a library in `Lib/python313`
      (with its `dist-info`), then deletes the dependency's folder and the loader and says so in a message; plugins import
      the library in the same session (`package_control_migrates_package_control_3_dependencies`, no network; also tried
      in the window). The paths plugins get have the platform's separators (`plugin_host.py::load`, and
      `CODELIBRE_TEXT_DATA` in `app_directories.rs`): Package Control prefixes them with `\\?\`, after which `/` is no
      separator, and with `C:/…` it could not list `Installed Packages`.
- [x] **Removed modules**: `assets/python/shims/` (written to `Lib/python/shims`, last on `sys.path`, so a Python that still
      has them uses its own): `imp` (gone in 3.12), `asyncore` / `asynchat`, `distutils` (`version`, `util`,
      `dir_util`, `file_util`, `spawn`, `sysconfig`), and `pipes`, `cgi`, `imghdr` and `uu` (3.13).
- [x] **Removed names**: `assets/python/compat.py`, applied before any plugin is imported: the `collections` ABCs (3.10),
      `inspect.getargspec` / `formatargspec` (3.11), `unittest`'s CamelCase methods, `configparser.SafeConfigParser` /
      `readfp` and `ssl.wrap_socket` / `match_hostname` (3.12), `cgi.escape` / `unescape` / `parse_qs` (3.8), `time.clock`
      (3.8), `typing.ByteString` / `io` / `re` (3.13 / 3.14), `threading`'s old names (3.9) and `pkgutil.ImpImporter` /
      `find_loader` / `get_loader` (3.12); `asyncio.get_event_loop()` makes a loop again as in 3.8 (3.14).
      `importlib.metadata.entry_points()` answers by group again (`.get`, `[group]`, `keys`), as 3.8's dict did, and takes
      `group=` on 3.9: Markdown needs both.
- [x] **Python 3.13 bundled** (2026-11-06): every installer carries its Python, so plugins and Package Control work
      offline with nothing else installed (the release builds download it, users never do). 3.13 is the Python of
      Sublime's current plugin host (build 4201 on): `sublime.version()` is 4201, Package Control installs libraries
      for it in `Lib/python313`, compiled ones too, and packages are written and tested for it. Python has no LTS:
      3.13 gets security fixes until October 2029; moving to a newer one is changing `fetch_python.sh` (version,
      release, hashes) and checking `sublime.version()` maps it. Plugins written for 3.8 run on it through `compat.py`
      / `shims/` and the `Lib/python38` libraries kept on `sys.path` (see above).
      It is a relocatable CPython 3.13.18 (python-build-standalone, `install_only_stripped`, SHA-256 pinned in
      `packaging/python/fetch_python.sh`, without tests / IDLE / Tk: about 60 MB), native for x64 and ARM64 on Windows,
      macOS and Linux. `find_interpreter` uses it after the `python_interpreter` setting (`bundled_interpreter`):
      `<exe folder>/python` (Windows, the portable archives), `Contents/Resources/python` (macOS app, each Mach-O signed
      ad hoc), `../lib/codelibre-text/python` (Linux `.deb` / `.rpm`, `install.sh`), and `target/<profile>/python` for a
      development build and its tests (`sh packaging/python/fetch_python.sh target/debug/python`; CI does it; the script
      skips the download when `python/BUILD` already names that build). Without it the installed Python is used: a 3.13
      first (also in the Mac folders a Finder-opened app's `PATH` lacks), then `python3`; `sublime.version()` reports the
      build whose host runs that Python (4203 for 3.14+, 4201 for 3.13, else 4200) and `compat.py` covers Apple's
      `/usr/bin/python3` (3.9 with LibreSSL: `allow_post_handshake_auth`).
- [ ] Python 3.3 / 3.8 hosts: not bundled; every package runs on the one Python. `.python-version` is not read;
      `"disable_plugin_host_3.3": true` makes Package Control install every package's libraries for it.
- [x] **Package Control runs** (4.2.8, copied by hand to `Installed Packages`): loads, lists the 4726 packages of the
      channel, installs a color scheme (`.sublime-package` + `installed_packages`), which shows after Reload Packages.
      It needed: the paths inside archives (above); `executable_path()` next to a `Packages` folder: Sublime's install,
      else an empty `Lib/Packages`, because it drops from `ignored_packages` any package it does not find, and would
      enable Vintage; `Lib/python38` (on `sys.path`) and `Lib/python33`; `Packages/Default/*` built-in defaults as
      resources; `Settings.settings_id`.
- [x] **Package Control bootstrap**: `install_package_control` (Tools > Install Package Control…, the palette) asks
      first (it downloads and runs code), then downloads `PACKAGE_CONTROL_URL` (`use_cases/src/packages.rs`: the latest GitHub release; packagecontrol.io still serves 3.4.1, which fails on Python 3.11+) into our
      `Installed Packages` through `DownloadGateway` (`curl_downloads.rs`: `curl` in a thread, to `<file>.download`, renamed
      when complete) and reloads the packages when it arrives (`Editor::poll_downloads`, every frame). It says
      "installed" only if Package Control's commands loaded; otherwise "downloaded but did not load (Help > Package Load
      Errors)". Download failures (curl's message) also go to the console. A reload with load errors says how many
      instead of the counts, so a package Package Control installs and that fails to load is noticed.
- [x] **`Installed Packages` is watched**: `check_installed_packages` (with `check_external_changes`, every 1.5 s) reloads
      the packages when a `.sublime-package` there is added, replaced or removed and the folder held still for one check
      (and the package folders of our `Packages`, below).
- [x] **Reloading keeps the plugin host**: a `load` to a running host (same `python_interpreter`) compares each package's
      `.py` files (`package_signature`) and unloads (`plugin_unloaded`, its modules, listeners, `add_on_change`
      callbacks) the packages gone or changed, then loads the new ones; commands are rebuilt in load order and `on_init`
      reaches only the new listeners, while view and text listeners of a new package attach to the open views. So a
      Package Control thread still installing goes on when the first package arrives. The editor keeps the plugins'
      settings files and watches unless the host started again (`PluginHostGateway::load` answers that). Left alone,
      as in Sublime: the timers of an unloaded package, kept packages that imported a changed one's modules.
- [x] **A package with plugins and libraries** (2026-11-04, MarkdownPreview 2.8.2 with Markdown, pygments, pymdown-extensions,
      PyYAML): installed (the libraries to `Lib/python38`), its `markdown_preview` command makes the HTML in a new view,
      upgraded (`upgrade_package`) and removed (`remove_package`, which also removes the orphaned libraries); the
      packages reload by themselves each time. `package_control_installs_upgrades_and_removes_a_package_with_plugins`.
- [x] **Our `Packages` is watched too**: `check_installed_packages` also compares its folders and the date of each one's
      `package-metadata.json` (Package Control writes it last), so a package it unpacks there (`.no-sublime-package`)
      reloads the packages when installed, upgraded or removed. The `ignored_packages` change around an install was
      not enough: it can last less than one 1.5 s check. Other files there (the `User` settings, a plugin edited by
      hand) are not watched: saving a `.py` reloads its package already.
- [x] **Popular packages load** (2026-11-04, `package_control_installs_the_listed_packages_at_start`): Package Control
      installs at start what `installed_packages` lists (`satisfy_packages`), and A File Icon, AdvancedNewFile,
      BracketHighlighter, ColorHelper, DocBlockr, Emmet, FileDiffs, GitGutter, LSP, MarkdownEditing, Origami,
      PackageResourceViewer, Pretty JSON, SideBarEnhancements, SublimeLinter, Terminus, Text Pastry and TrailingSpaces
      load with no errors (with 11 libraries for 3.14). Loading is not using: most of what they show needs phase C.
      It needed the API enums (`RegionFlags`, `PopupFlags`, `KindId`… of 4132, LSP), our `Default.paste_from_history`
      (Terminus) and the libraries for our Python (above).
- [x] **Package Control upgrades itself** (`package_control_upgrades_itself_then_the_other_packages`): `upgrade_all_packages`
      upgrades it alone (it is in `ignored_packages` meanwhile, so its plugins unload and load again, while its thread
      goes on), then the other packages the next time.
- [x] **A removal deferred to the next start** (Windows locked files, `package_control_removes_a_locked_package_at_the_next_start`,
      no network): a file nobody may delete or move makes Package Control leave `package-control.cleanup` in the package's
      folder ("scheduled clean up on next restart"); the next start deletes the folder and enables what it had disabled.
- [x] **Local channels and repositories** (2026-11-06, `package_control_installs_from_local_repositories_and_channels`, no
      network): `add_repository` / `add_channel` with `file:///` URLs are saved in its settings and their packages offered;
      a package with `.no-sublime-package` is unpacked to `Packages/<name>`, its plugin loads, is upgraded in place and
      removed; a package named like one Sublime ships (`Diff`) overrides it and `revert_package` removes it. Nothing of
      ours needed changing.
- [x] **`Default` modules**: Sublime's `Default` is not loaded, so `assets/python/Default/` (written to `Lib/python/Default`)
      has our own modules packages import: `paste_from_history` (`ClipboardHistory`, `g_clipboard_history`) and `exec`
      (`ExecCommand`, `AsyncProcess`, `ProcessListener`: build plugins subclass them; the process runs in the plugin host,
      its output goes to the `exec` output panel with its `result_*` settings, `kill`, `interactive` without an input view,
      errors kept in `errs_by_file` but not drawn inline). They are only imported, not loaded as plugins: their listeners
      would make every copy / paste a command for the hooks, and `set_commands` skips classes of `Default.*` (the
      editor's own `exec` stays; a plugin's subclass is a command under its own name). `window.create_io_panel` /
      `find_io_panel` answer the output panel and, with an `on_input` callback, the field that types into its process
      (Enter calls `on_input(text)` and keeps the field open, Esc closes it; the field shares the panel, focused through
      `window.focus_view`). Missing: the `paste_from_history` command and its listener.
- [x] **Warnings**: `DeprecationWarning` and `SyntaxWarning` (Python 3.12's invalid escapes in old plugins) are not
      shown in the console, as in Sublime, where plugins are not `__main__`.
- [x] **Package Control's disabling**: Disable / Enable Package and the steps around Upgrade / Remove only change
      `ignored_packages` (and `in_process_packages` in `Package Control.sublime-settings`, `package_disabler.py`), expecting
      the editor to unload / load those packages at once. `check_ignored_packages` (with `check_external_changes`, every
      1.5 s) reloads the packages when the setting is not the one they were loaded with; only the plugins of the packages
      concerned reload (see reloading above).
- [x] **Package settings**: `edit_settings` (Preferences > Package Settings > <package> > Settings, a package's palette
      entries) opens the package's own file as the read-only left view (`load_resource`, also inside archives; our
      `Default` package's are built in) and the `User` copy next to it, as tabs (not side by side in a group). A
      package's keymap no longer shows ours. Preferences > Package Settings is a real submenu packages merge into (`id`
      `package-settings`); the menu presenter hides submenus left empty and separators at an edge or doubled.
- [x] **A view's `syntax` setting** (2026-11-06, found by installing Pretty JSON from the window): every view has
      it, the syntax's resource (`SyntaxGateway::syntax_path`; built-in ones get Sublime's path, `Packages/Text/Plain
      text.tmLanguage`), so plugins can take its file name; setting it changes the view's syntax. Without it Pretty
      JSON formatted and then failed (`os.path.basename(None)`).
- [x] `$packages` / `$platform` without braces (Package Control's `open_file` of `$packages/User/Package
      Control.user-ca-bundle`): `PackagePaths::expand`, `gateways::expand_variable`. `semi_transient` is ignored.
- [x] `window.set_view_index` / `set_sheet_index` move the tab to a group and an index (`Editor::move_document`).
      Package Control moves its message view with them.
- [x] **Scopes** from the syntax engine: `SyntaxGateway::line_scopes` parses one line again from the parser state the
      highlighting cache keeps for its start (`SyntaxEngine::line_scopes`); the use cases (`use_cases/src/scopes.rs`)
      turn that into `view.scope_name` (with Sublime's trailing space), `match_selector`, `score_selector`,
      `extract_scope` (the run of text with the same scope, across lines), `find_by_selector`, `expand_to_scope`,
      `extract_tokens_with_scopes`. Selectors (`entities/src/selector.rs`, also `sublime.score_selector` and build
      systems): `,` / `|`, ` - `, descendants, parentheses ignored. Plain text and files over 4 MB are the base scope.
- [x] The **`selector` key binding context** (`keyboard_controller.rs`): it matches when the scope at some caret does.
      Language keymaps now apply when their other contexts are known (`insert_snippet` expands `$SELECTION` and puts the
      caret on the first field: `text_commands::insert_snippet`); those with `preceding_text` / `following_text` still do not.
- [x] **`view.style()` / `style_for_scope()`** from the color scheme in use (`SyntaxGateway::scheme_style` /
      `scope_style`, `SyntaxEngine`: syntect's `style_mod_for_stack` on the whole scope stack): globals and a scope's
      `foreground`, `background` (when a rule sets one), `bold` / `italic` / `underline`, as `#rrggbb` or `#rrggbbaa`.
      The `source_*` keys are empty; the global `color_scheme`, not a view's own.
- [x] **`view.export_to_html`** (`use_cases/src/html_export.rs`): the regions asked for (or the whole view) as HTML,
      each run in a `<span>` with the scheme's color, background and bold / italic / underline; a `<pre>` keeps the
      lines and spaces; `enclosing_tags` adds the `<div>` with the background color (and `font_size` / `font_family`);
      `minihtml` uses the same shapes, which our minihtml reads. `&`, `<` and `>` are escaped.
- [x] `meta_info` (`.tmPreferences`), `symbol_regions` / `indexed_symbols` / `indexed_references`, `context_backtrace`
      (`entities/src/meta_info.rs`), and `match_all` in key binding contexts. `window.symbol_locations` (and the
      deprecated `lookup_symbol_in_index` / `_in_open_files`, `lookup_references_in_index` / `_in_open_files`) search
      the open files (`use_cases/src/scopes.rs::symbol_locations`); the index on disk is not kept, so `INDEX` answers
      the saved open files.
- [ ] Scopes, still missing: Sublime's exact `extract_scope` and score values; `find_by_selector` and `extract_scope`
      parse the whole file every call.
- [x] **Completions** (`use_cases/src/completions.rs`, `entities/src/completion.rs`, drawn by `editor_view.rs`): the
      auto-complete list opens while typing a word where `auto_complete_selector` matches (or a character of
      `auto_complete_triggers` where its selector does; `auto_complete`, `auto_complete_size_limit`), or with the
      `auto_complete` command (Ctrl+Space; `api_completions_only`, `snippets_only`, `commit_single`). It filters itself
      fuzzily as the word grows (the first typed char must start a word of the trigger; the matched chars are drawn),
      Up / Down / Page Up / Page Down move in it (`keyboard_controller`), Enter (unless `auto_complete_commit_on_tab`)
      and Tab commit (`commit_completion`, the `auto_complete_visible` context), Esc (`hide_auto_complete`) or a caret
      outside the word close it. Sources, in that order: `on_query_completions` of every listener, merged by
      `sublime_plugin.query_completions` (lists, `(list, flags)`, `CompletionItem`s with kind badge, annotation and
      minihtml details, `CompletionList`s filled later from any thread by `set_completions` → `completion_list_set`);
      the packages' `.sublime-completions` and snippets with a `tabTrigger` whose scope matches; the file's words,
      nearest first. `INHIBIT_WORD_COMPLETIONS`, `INHIBIT_EXPLICIT_COMPLETIONS`, `DYNAMIC_COMPLETIONS` (asked again as
      the word grows), `INHIBIT_REORDER`; formats text, snippet (`insert_snippet`: the first field selected) and
      command (the command runs); `KEEP_PREFIX`. Tab after a trigger typed in full inserts it (`tab_completion`,
      `insert_best_completion`). `view.is_auto_complete_visible()`, `view.extract_completions()`.
- [x] The rest: snippet fields after the first (`next_field` / `prev_field` / `clear_fields`), several carets
      (`complete_at_each`), `auto_complete_delay`, `auto_complete_use_history`, `auto_complete_cycle`,
      `replace_completion_with_next_completion`, the words of the other open views, `rhs_empty` triggers.
- [x] **Popups** (`use_cases/src/plugin_popups.rs`): `show_popup` (location, `max_width` / `max_height`, `on_navigate`,
      `on_hide`), `update_popup`, `is_popup_visible`, `hide_popup`. One popup, drawn by `editor_view.rs` in an egui `Area`
      below its char (above when there is no room), scrolling past `max_height`. It goes when the view's caret moves
      (unless `KEEP_ON_SELECTION_MODIFIED`), another view becomes active, with Esc (`keyboard_controller::key_pressed`,
      before the keymap), when the mouse leaves it (`HIDE_ON_MOUSE_MOVE*`) or another popup replaces it; `on_hide` runs at
      the next `poll_plugins` (`PluginState::pending_callbacks`).
- [x] **minihtml** (`entities/src/minihtml.rs`, drawn by `views/minihtml_view.rs`): `b`/`strong`, `i`/`em`, `u`, `code` /
      `tt` / `pre`, headings, `a href`, `br` and blocks (`p`, `div`, `li` with a bullet…), `color` / `background-color` /
      `font-weight` / `font-style` in `style`, `<font color>`; colors `#rgb[a]`, `#rrggbb[aa]`, `rgb[a]()`, names,
      `var(--foreground|background|<color>ish|accent)` and Sublime's `color(…)` (adjustments ignored); `<style>` /
      `<script>` skipped, entities decoded. Missing: CSS rules from `<style>` (classes), padding / margins / borders,
      images, `font-size`, tables as tables.
- [x] **Phantoms**: `add_phantom` / `erase_phantoms` / `erase_phantom_by_id` / `query_phantom(s)`, `PhantomSet.update`
      (an equal phantom where edits moved it keeps its id, as in Sublime); kept in the `Document` (`Phantom`), moved by
      its edits like regions; links call `on_navigate`. Every layout is drawn where Sublime draws it: `LAYOUT_INLINE`
      pushes the text after its point aside, `LAYOUT_BELOW` / `LAYOUT_BLOCK` make room under the line.
- [x] **`on_hover(view, point, hover_zone)`**: the view reports where the mouse rested for 0.5 s (text, gutter or the
      margin after a line, not over a popup), once per place, only when a listener defines it (a query, so
      `ViewEventListener`s hear it too), also over phantoms.
- [ ] `on_hover` still missing: over annotations (`add_regions`' `annotations`).
- [x] **Groups and layouts** (`use_cases/src/groups.rs`, `entities/src/layout.rs`, drawn by `app.rs::show_group`): each
      document is in a group (`Document::group`), the tabs of a group are its documents in window order, and a group shows
      the one used last. `set_layout` (View > Layout, Alt+Shift+1…5/8/9), `focus_group` (Ctrl+1…9),
      `move_to_group` (Ctrl+Shift+1…9), `focus_neighboring_group` / `move_to_neighboring_group` (Ctrl+K Ctrl+arrows), View >
      Focus Group / Move File To Group. Fewer groups move their files to the last one; files open in the focused group;
      the focused group may be empty (no active view, as in Sublime); `next_view` and `select_by_index` stay in the group.
      Clicking a group's text, tab strip or empty area focuses it. The session keeps the layout and each file's group.
      API: `num_groups`, `active_group`, `focus_group`, `layout` / `get_layout` / `set_layout`, `views_in_group`,
      `active_view_in_group`, `get_view_index` / `set_view_index` (group and index), `sheets_in_group`,
      `active_sheet_in_group`, `move_sheets_to_group`, `Sheet.group()`, `open_file(group=)`.
- [x] The rest: dragging the borders between groups and tabs between them, `new_pane` / `close_pane` /
      `set_max_columns`, the side bar's open files by group, `transient` views (`window.transient_view_in_group`).
- [x] **Several windows and clones** (`new_window`, `clone_file`, `view.clones()`, `view.is_primary()`): each window has
      its own views, layout, groups, folders, output panels and settings; a clone keeps its selections, regions,
      phantoms and scroll apart from the `Document` (edits and saves are shared).
- [x] **`window.new_html_sheet`** (`HtmlSheet`): a read-only scratch tab whose text is the plain text of the minihtml
      (`entities/src/minihtml.rs`), in the group it asks for. `HtmlSheet.set_contents` replaces that text; links are not
      clickable.
- [x] `sublime_plugin.reload_plugin` / `unload_plugin`. (Saving a plugin reloads only its package: see reloading above.)

`crates/frameworks/tests/sublime_packages.rs` (`-- --ignored`) loads the packages of the Sublime Text installed here:
68 syntaxes, 34 color schemes, 20 build systems, their plugins, no errors (2026-10-30). Language keymaps load; their
bindings apply through the `selector` context (above) when they need no other unknown context.

Verification for phase C: `package_control_loads_in_a_scratch_profile` in `crates/frameworks/tests/sublime_packages.rs`
(`-- --ignored`, needs the network) copies the installed Package Control to a temporary profile and installs a color
scheme, then disables and enables it with Package Control's commands (the color scheme goes and comes back), and
opens Preferences > Package Settings > Package Control > Settings. `package_control_installs_upgrades_and_removes_a_package_with_plugins`
(same file, `-- --ignored`, needs the network, runs third-party code) installs MarkdownPreview, runs its command, upgrades
and removes it; `CODELIBRE_TEST_PACKAGE`, `CODELIBRE_TEST_COMMAND` and `CODELIBRE_TEST_ARGS` try another package.
`package_control_installs_the_listed_packages_at_start` installs 18 popular packages (or `CODELIBRE_TEST_PACKAGES`,
comma-separated) and fails on any plugin load error: the way to find what the next package needs.
`package_control_upgrades_itself_then_the_other_packages` checks `upgrade_all_packages`.
`package_control_installs_from_local_repositories_and_channels` (no network) checks `add_repository`, `add_channel`,
unpacked packages and `revert_package`. `package_control_migrates_package_control_3_dependencies` (no network) checks the migration of a Package Control 3 profile,
`package_control_removes_a_locked_package_at_the_next_start` (Windows, no network) a removal deferred to the next start.
