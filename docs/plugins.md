# Python plugins (Sublime Text API)

Status: **phase A done** (2026-10-26); **phase B in progress** (core done 2026-10-28: request ids, calls
outside commands, real timers, view events; then quick and input panels; 2026-10-29 dialogs, resources,
scratch views, `add_on_change`, output panels, statuses and regions drawn, `view.show`, `on_query_context`, command hooks;
2026-10-30 output panel results, build systems, the input panel's view). The rest of B and phase C below are the pending work, in order.
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
editor → host  {"op":"load","packages":[{"name":"MyPkg","dirs":["…"]}],"paths":{"packages":…,"installed_packages":…,"cache":…,"executable":…}}
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
- Saving a `.py` file in a package reloads the plugins (`reload_plugins`); Preferences > Reload Packages too.

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
- Stand-ins (so plugins do not crash, but nothing visible happens yet): `scope_name` is `""`, `match_selector` is false.
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
      disk), `on_init(views)` (after `Editor::start` and after every reload of the plugins), `on_exit()` (the window
      closes, `Editor::plugin_exit`).
- [x] **`TextChangeListener`** (`on_text_changed`, `_async`, `on_revert`, `on_reload`): `Document::log_text_changes`
      keeps a copy of the text and compares it after each edit (the widget edits in place), once a frame the
      changes go in an `on_text_changed` query. Several carets count as one change from the first to the last.
      The columns of `HistoricPosition` are chars, also in `col_utf8` / `col_utf16`.
- [ ] Events still missing: `on_clone`, `on_new_window` and `applies_to_primary_view_only` wait for clones and several windows (phase C),
      `on_hover` for popups (phase C).
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
      items are strings, lists (caption, detail lines) or `QuickPanelItem`s (detail lines joined with ` · `,
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
      Build System list (`$build_systems`), `from Default.exec import ExecCommand`, and the builds inside Sublime's zipped
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
      would take every key) and their plugins are not loaded. A change needs `reload_packages`; Package Control's
      toggling while upgrading is not followed yet.
- [ ] **Libraries / dependencies**: ST4 `Lib/python38` (and Package Control's `dependencies.json` / `libraries`) on `sys.path`.
- [ ] **Python version**: plugins target 3.8 (or 3.3 via `.python-version`); we run whatever Python 3 is installed (3.14 here).
      Provide shims for removed modules (e.g. `imp`, gone in 3.12 — Sublime ships one for its 3.14 host) next to `sublime.py`.
- [x] **Package Control runs** (4.2.8, copied by hand to `Installed Packages`): loads, lists the 4726 packages of the
      channel, installs a color scheme (`.sublime-package` + `installed_packages`), which shows after Reload Packages.
      It needed: the paths inside archives (above); `executable_path()` next to a `Packages` folder: Sublime's install,
      else an empty `Lib/Packages`, because it drops from `ignored_packages` any package it does not find, and would
      enable Vintage; `Lib/python38` (on `sys.path`) and `Lib/python33`; `Packages/Default/*` built-in defaults as
      resources; `Settings.settings_id`.
- [ ] **Package Control bootstrap**: `install_package_control` downloads `Package Control.sublime-package` into
      `Installed Packages` (ask the user: it downloads and runs code). There is no such command yet.
- [ ] Packages installed by Package Control need Reload Packages: nothing watches `Installed Packages`, and a plugin
      package's plugins start only then. Not tried yet: a package with plugins or libraries, upgrades, removal.
- [ ] **Scopes** from the syntax engine (`syntax_engine.rs` / syntect parse state): `scope_name`, `match_selector`,
      `score_selector`, `extract_scope`, `style_for_scope`, `sublime.score_selector`, `view.meta_info`, `view.symbol_regions`.
- [ ] **Completions**: `on_query_completions`, `CompletionItem`, `view.auto_complete` (needs an auto-complete UI first).
- [ ] **Popups, phantoms, annotations, hover** (`show_popup` with minihtml, `PhantomSet`, `on_hover`).
- [ ] **Several windows, clones, groups/layouts** (`new_window`, `set_layout`, `move_to_group`).
- [ ] Reload only the plugin that was saved (now every plugin reloads), and `sublime_plugin.reload_plugin`.

`crates/frameworks/tests/sublime_packages.rs` (`-- --ignored`) loads the packages of the Sublime Text installed here:
68 syntaxes, 34 color schemes, 20 build systems, their plugins, no errors (2026-10-30). Language keymaps load, but their
bindings need the `selector` context, so they stay inactive until **Scopes** below.

Verification for phase C: `package_control_loads_in_a_scratch_profile` in `crates/frameworks/tests/sublime_packages.rs`
(`-- --ignored`, needs the network) copies the installed Package Control to a temporary profile and installs a color
scheme. What is left is a plugin package and running its commands.
