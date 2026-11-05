# Python plugins (Sublime Text API)

Status: **phase A done** (2026-10-26); **phase B in progress** (core done 2026-10-28: request ids, calls
outside commands, real timers, view events; then quick and input panels; 2026-10-29 dialogs, resources,
scratch views, `add_on_change`, output panels, statuses and regions drawn, `view.show`, `on_query_context`, command hooks;
2026-10-30 output panel results, build systems, the input panel's view; 2026-11-02 stand-ins for the rest of the API,
`imp` shim, `install_package_control`, `Installed Packages` watched, scopes and the `selector` context; 2026-11-04 a
package with plugins and libraries installed, upgraded and removed by Package Control; 18 popular packages load,
libraries for the Python we run, Package Control upgrades itself; 2026-11-05 Package Control 3's dependencies, a
removal deferred to the next start, `Default.exec`, `view.style` / `style_for_scope`, popups, phantoms, `on_hover`,
auto-complete and plugins' completions, groups and layouts). The rest of B and phase C below are the pending work, in order.
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

- Positions are char indices (like `Selection`); view id = `DocumentId`; there is one window (id 1).
- `view.sel()` sends the whole list on every change (`view_selection_set`); the **last** region becomes
  the primary selection the text widget shows. A cleared selection stays on the Python side until refilled.
- Settings targets: `["view", id]`, `["file", "Name.sublime-settings"]`, `["window", 1]`.
  `Preferences.sublime-settings` changes apply at once (session overrides) and are written on `save_settings`.
- Plugins load from every package folder with top-level `.py` files (ours and, with `import_sublime_packages`,
  Sublime's, except its `Default`), each package imported as a Python package so relative imports work.
- Settings: `python_plugins` (default true), `python_interpreter` (default: `py -3`, `python3`, `python`, `uv python find`).
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
- Stand-ins (so plugins do not crash, but nothing visible happens yet), for what phase C will implement:
  `meta_info`, symbols, `show_popup_menu` (a quick panel), `CompletionItem` / `CompletionList`,
  folding, the view's geometry (`text_to_layout`…, a fixed 8×16 font), groups / layouts / sheets (`Sheet`), the symbol
  index, `list_syntaxes` / `find_syntax_*`. (Scopes, styles, popups and phantoms were stand-ins too; phase C made them real.)
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
- [ ] Events still missing: `on_clone`, `on_new_window` and `applies_to_primary_view_only` wait for clones and several windows
      (phase C). (`on_hover` is done, with popups: phase C.)
- [x] Built-in commands that edit text (`EDITING_COMMANDS` in `command_controller.rs`) run at once inside an event or
      query, on the view that asked (`Editor::set_event_command_runner`, set by `compose()`), through the hooks, so a
      formatter's `view.run_command(...)` in `on_pre_save` reaches the file.
- [ ] Built-in commands that need `ViewState` (panels, the widget's undo / clipboard, `save`, `close`…) still wait for the
      next frame when run from an event, and then on the active view; running them at once means moving their
      `ViewState` changes out of those arms, or `save` / `close` into the controller.
- [x] **`on_query_context`**: a key binding condition the keyboard controller's context does not know goes to the
      listeners (`Editor::press_key` wraps the context; the keymap is set aside while they answer). The operator
      becomes `sublime.OP_*`; the first `True` / `False` wins, `None` from everyone means the binding does not apply.
      `match_all` is always false.
- [x] **`on_text_command` / `on_window_command`**: `command_controller::run` asks before every command; a `(name, args)`
      answer runs instead (once: the new command is not asked about again). `on_post_text_command` /
      `on_post_window_command` after it, with the command that ran. Plugin commands go by their kind (application
      commands have no hook); built-ins by `WINDOW_COMMANDS`, the rest are text commands (only with an open view).
      Built-in commands a listener runs while answering are handled as in events.
- [x] Hooks around the widget: while a listener of text commands exists and the text has the focus, the keys of undo,
      redo, select all, copy, cut and paste run as commands (`hooks_widget_edits` in `keyboard_controller.rs`), so a
      listener can replace them; `on_post_text_command` waits for the frame in which the widget did the edit
      (`ViewState::widget_edits_in_progress`). Without listeners the keys go straight to the widget, as always.
- [ ] Hooks, still missing: typing, deleting and caret movement (`insert`, `left_delete`, `move`…) stay the widget's and
      are not seen. (`match_all` reaches `on_query_context` from the binding; `on_post_*_async` run on the async thread.)
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
- [x] **Motions as commands**: `move` (`by`: `characters`, `lines`, `words`, `word_ends`, `subwords` and `subword_ends`
      as words; `forward`, `extend`) and `move_to` (`bol`, `eol`, `hardbol`, `hardeol`, `bof`, `eof`, `extend`), every
      selection, in tabs, panels and events. Key bindings to them still go to the widget. Missing: `by: pages` / `stops`,
      `to: brackets`, and `bol` / `eol` of wrapped lines (they go to the logical line's).
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
- [ ] Builds, still missing: `$project*` variables and a project's `build_systems` (projects are not read), the Tools >
      Build System list (`$build_systems`), and the builds inside Sublime's zipped
      packages (`Python`, `C++`…), which need `.sublime-package` (phase C). There is no Ctrl+Break: egui has no such key.
- [ ] Output panels, still missing: the panel's own syntax highlighting / `word_wrap`, selections drawn, `unlisted`,
      built-in commands other than `VIEW_COMMANDS` (`editing.rs`: `move`, `move_to`, `left_delete`, `right_delete`,
      `select_all`, `insert`) and `append` in a panel (the command controller only knows tabs), the console as a panel (it is a tab), results in Find Results (find in files is not implemented).
- [x] **Command input handlers**: a command chosen in the palette (or `show_overlay` with `command`) asks
      `command_input`; when `input()` returns a handler, `sublime_plugin._CommandInput` asks in the existing panels
      (`ListInputHandler` in the quick panel, `TextInputHandler` in the input panel, with `validate`, `confirm`,
      `cancel`, `next_input`, `BackInputHandler`, `sublime.ListInputItem`) and runs the command itself. Key bindings
      and menus run the command directly. `preview` goes to the status bar.
- [ ] Input handlers, still missing: they are not inside the palette as in Sublime (no breadcrumb, `Backspace` does
      not go back to the previous handler), `initial_selection`, `want_event`, HTML previews.
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
- [x] **Regions drawn**: `view.add_regions(key, regions, scope, icon, flags)` / `get_regions` / `erase_regions` keep a
      `RegionSet` in the `Document`, moved by its edits (`replace_range`). Drawn under the text in the scope's color
      (the color scheme's rule, or a built-in color for `region.<color>ish`): filled and outlined unless
      `DRAW_NO_FILL` / `DRAW_NO_OUTLINE`, solid / stippled / squiggly underlines, `DRAW_EMPTY` as a bar, `HIDDEN` not
      drawn; `dot` / `circle` / `bookmark` icons in the gutter (any other icon is a dot).
- [x] Regions move with every edit, also the widget's: `add_regions` keeps a copy of the text, compared in
      `Document::note_text_changed`. An edit with several selections is one change from the first caret to the
      last, so regions between the carets collapse.
- [ ] Regions, still missing: annotations, `on_navigate`, `PERSISTENT`,
      `DRAW_EMPTY_AS_OVERWRITE`, icon files, the minimap.
- [x] **Resources**: `sublime.find_resources(pattern)` (file name with `*` / `?`, as `Packages/<pkg>/<path>`, each
      once, ours over Sublime's), `load_resource` (UTF-8, `\n` line endings), `load_binary_resource` (sent as hex);
      a missing one raises `FileNotFoundError`. `decode_value` / `encode_value`. Zipped packages are phase C.
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
      `0_package_control_loader.sublime-package` is in our `Installed Packages`): each to a library in `Lib/python314`
      (with its `dist-info`), then deletes the dependency's folder and the loader and says so in a message; plugins import
      the library in the same session (`package_control_migrates_package_control_3_dependencies`, no network; also tried
      in the window). The paths plugins get have the platform's separators (`plugin_host.py::load`, and
      `CODELIBRE_TEXT_DATA` in `app_directories.rs`): Package Control prefixes them with `\\?\`, after which `/` is no
      separator, and with `C:/…` it could not list `Installed Packages`.
- [x] **Removed modules**: `assets/python/shims/` (written to `Lib/python/shims`, last on `sys.path`, so a Python that still
      has them uses its own): `imp` (gone in 3.12) and `distutils.version` (`LooseVersion`, `StrictVersion`).
- [ ] **Python version**: plugins target 3.8 (or 3.3 via `.python-version`); we run whatever Python 3 is installed (3.14 here).
      More shims as plugins need them (`asyncore` / `asynchat`, the rest of `distutils`). Done in `plugin_host.py`:
      `importlib.metadata.entry_points()` answers by group again (`.get`, `[group]`, `keys`), as 3.8's dict did
      (`restore_entry_points_by_group`; Markdown needs it). Libraries for 3.8 run on 3.14 only while they avoid what
      later Pythons removed; compiled ones (`cp38` wheels) cannot.
      On macOS that is often Apple's `/usr/bin/python3` (3.9, LibreSSL), whose `SSLContext.post_handshake_auth` cannot be
      set: `plugin_host.py::allow_post_handshake_auth` makes it a no-op, or every Package Control download fails.
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
- [ ] Not tried yet with the real Package Control: a `.no-sublime-package` package, `revert_package`,
      `add_channel` / `add_repository`.
- [x] **`Default` modules**: Sublime's `Default` is not loaded, so `assets/python/Default/` (written to `Lib/python/Default`)
      has our own modules packages import: `paste_from_history` (`ClipboardHistory`, `g_clipboard_history`) and `exec`
      (`ExecCommand`, `AsyncProcess`, `ProcessListener`: build plugins subclass them; the process runs in the plugin host,
      its output goes to the `exec` output panel with its `result_*` settings, `kill`, `interactive` without an input view,
      errors kept in `errs_by_file` but not drawn inline). They are only imported, not loaded as plugins: their listeners
      would make every copy / paste a command for the hooks, and `set_commands` skips classes of `Default.*` (the
      editor's own `exec` stays; a plugin's subclass is a command under its own name). `window.create_io_panel` /
      `find_io_panel` answer `(output panel, None)`. Missing: the `paste_from_history` command and its listener.
- [x] **Warnings**: `DeprecationWarning` and `SyntaxWarning` (Python 3.12's invalid escapes in old plugins) are not
      shown in the console, as in Sublime, where plugins are not `__main__`.
- [x] **Package Control's disabling**: Disable / Enable Package and the steps around Upgrade / Remove only change
      `ignored_packages` (and `in_process_packages` in `Package Control.sublime-settings`, `package_disabler.py`), expecting
      the editor to unload / load those packages at once. `check_ignored_packages` (with `check_external_changes`, every
      1.5 s) reloads the packages when the setting is not the one they were loaded with; only the plugins of the packages
      concerned reload (see reloading above).
- [x] **Package settings**: `edit_settings` (Preferences > Package Settings > <package> > Settings, a package's palette
      entries) opens the package's own file as the read-only left view (`load_resource`, also inside archives; our
      `Default` package's are built in) and the `User` copy next to it, as tabs (no groups). A package's keymap no longer
      shows ours. Preferences > Package Settings is a real submenu packages merge into (`id` `package-settings`); the menu
      presenter hides submenus left empty and separators at an edge or doubled.
- [x] `$packages` / `$platform` without braces (Package Control's `open_file` of `$packages/User/Package
      Control.user-ca-bundle`): `PackagePaths::expand`, `gateways::expand_variable`. `semi_transient` is ignored.
- [x] `window.set_view_index` / `set_sheet_index` move the tab (`Editor::move_document`; one group, so only the index
      counts). Package Control moves its message view with them.
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
- [ ] Scopes, still missing: `meta_info` (`.tmPreferences`), `symbol_regions` / `indexed_symbols`, `context_backtrace`,
      Sublime's exact `extract_scope` and score values, `match_all` in contexts; `find_by_selector` and `extract_scope`
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
- [ ] Completions, still missing: the snippet fields after the first (Tab to the next one), several carets (only the
      primary one is completed), `auto_complete_delay`, `auto_complete_use_history`, `auto_complete_cycle`,
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
      its edits like regions; links call `on_navigate`. Missing: every layout is drawn after the end of the line of the
      region's end, over the text: `LAYOUT_BLOCK` / `LAYOUT_BELOW` make no room under the line, `LAYOUT_INLINE` does not
      push the text aside.
- [x] **`on_hover(view, point, hover_zone)`**: the view reports where the mouse rested for 0.5 s (text, gutter or the
      margin after a line, not over a popup), once per place, only when a listener defines it (a query, so
      `ViewEventListener`s hear it too). Missing: annotations (`add_regions`' `annotations`), `on_hover` over phantoms.
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
- [ ] Groups, still missing: dragging the borders between groups, dragging tabs between them, `new_pane` / `close_pane`
      and `set_max_columns` (Python in Sublime's `Default`), the side bar's open files by group, `transient` views.
- [ ] **Several windows and clones** (`new_window`, `clone_file`, `view.clones()`): egui's viewports for windows; a clone
      needs the view (selection, scroll) apart from the `Document`.
- [ ] `sublime_plugin.reload_plugin`. (Saving a plugin reloads only its package: see reloading above.)

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
`package_control_migrates_package_control_3_dependencies` (no network) checks the migration of a Package Control 3 profile,
`package_control_removes_a_locked_package_at_the_next_start` (Windows, no network) a removal deferred to the next start.
