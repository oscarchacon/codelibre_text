# Python plugins (Sublime Text API)

Status: **phase A done** (2026-10-26). Phases B and C below are the pending work, in order.
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
| Loading, running, console, conversation loop (`converse`) | `crates/use_cases/src/plugins.rs` | Use Cases |
| The `sublime_api` functions the editor answers | `crates/use_cases/src/plugin_api.rs` | Use Cases |
| Edits from plugins move every selection (`Document::replace_range`) | `crates/entities/src/document.rs` | Entities |
| Plugin commands are the fallback of the command controller; `show_panel console` | `crates/interface_adapters/src/controllers/command_controller.rs` | Interface Adapters |
| Arbitrary `.sublime-settings` files (`load_settings_file`, `*_user_settings_file`) | `crates/interface_adapters/src/gateways/sublime_resources.rs` | Interface Adapters |
| Process, interpreter discovery, reader threads, 20 s timeout | `crates/frameworks/src/gateways/python_plugin_host.rs` | Frameworks |
| Host, transport, our own `sublime` / `sublime_plugin` modules | `assets/python/*.py` (embedded with `include_str!`, written to `%APPDATA%\codelibre_text\data\Lib\python` at start) | — |
| Fake host for tests (`PluginScript`, `plugin_call`, `World::with_plugin_commands`) | `crates/test_support/src/lib.rs` | tests |

`sublime.py` and `sublime_plugin.py` are **our own code**, written from the public API reference
(https://www.sublimetext.com/docs/api_reference.html). Do not copy Sublime's files from
`C:\Program Files\Sublime Text\Lib\python3*\` (proprietary license); reading them to learn a function's
behaviour is fine. Their `sublime_api` function names are a good guide for naming new functions.

### Protocol

```
editor → host  {"op":"load","packages":[{"name":"MyPkg","dirs":["…"]}],"paths":{"packages":…,"installed_packages":…,"cache":…,"executable":…}}
host → editor  {"op":"loaded","commands":[["hello","text"],…],"errors":["traceback",…]}
editor → host  {"op":"run","kind":"text|window|application","name":"hello","args":{…},"view":7,"window":1}
host → editor  {"op":"call","fn":"view_insert","args":[7,0,"hi"]}        (zero or more)
editor → host  {"op":"result","value":2}   or   {"op":"result","error":"the view is closed"}
host → editor  {"op":"log","text":"printed text"}                         (any time)
host → editor  {"op":"done","error":null}   or   {"op":"done","error":"Traceback …"}
editor → host  {"op":"exit"}
```

While the host waits for a `result` it may receive a nested `run` (a plugin's `run_command` of another
plugin command); it runs it and answers `done` before going on. The editor does the same in `converse`.
A plugin's `run_command` of a built-in command goes back through the command controller (the use case
receives it as a `CommandRunner` callback, because built-in commands live in an outer circle).

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
- Stand-ins (so plugins do not crash, but nothing visible happens yet): `set_timeout*` run right after the current
  command; `add_regions`/`set_status` are stored but not drawn; `scope_name` is `""`, `match_selector` is false;
  `show`/`show_at_center` do nothing; `ok_cancel_dialog` returns false.

## Phase B — events, panels, async (next)

Goal: plugins that react to the editor and show UI (linters, formatters on save, quick-panel pickers).

- [ ] **Request ids in the protocol** (`"id"` on `call`/`result`), so calls can also come from background threads.
- [ ] **Calls outside a command**: a non-blocking `PluginHostGateway::try_receive`; `app.rs` polls it every frame and
      serves calls (`converse` without waiting for `done`). Host side: `codelibre_rpc.call` from other threads must
      queue under a lock instead of raising (see `_main_thread` check in `codelibre_rpc.py`).
- [ ] **Real timers**: `set_timeout(cb, delay)` / `set_timeout_async` with delays — the host keeps a timer heap and runs
      callbacks from its main loop (it needs a `select`-style wait on stdin with a timeout; on Windows use a reader thread + queue).
- [ ] **Event listeners** (`EventListener`, `ViewEventListener`, `TextChangeListener`): host sends `{"op":"event","name":"on_modified","view":id}`.
      Fire from the use cases: `on_new`, `on_load` (open_file), `on_activated`/`on_deactivated` (activate),
      `on_pre_save`/`on_post_save` (saving.rs), `on_close` (close_documents), `on_modified`/`on_selection_modified`
      (from `app.rs` after the text widget changed the document; debounce), `*_async` variants after the sync ones.
- [ ] **`on_query_context`**: key binding contexts unknown to `KeyContext` ask the plugins (keyboard_controller → use case → host).
- [ ] **`on_text_command` / `on_window_command`**: let plugins rewrite or cancel commands before the controller runs them.
- [ ] **Quick panel**: `window.show_quick_panel(items, on_select, flags, selected_index, on_highlight)` → the
      `QuickPanel` view state with plugin items; the answer goes back as `{"op":"callback","id":…,"args":[index]}`.
- [ ] **Input panel**: `window.show_input_panel(caption, initial, on_done, on_change, on_cancel)` (a one-line panel like Find).
- [ ] **Output panels**: `window.create_output_panel(name)` returns a view (a hidden document) shown at the bottom by
      `show_panel {"panel": "output.<name>"}`; `find_output_panel`, `destroy_output_panel`. Build systems need it.
- [ ] **Command input handlers** (`input()` → `TextInputHandler`/`ListInputHandler`) in the command palette.
- [ ] **Command metadata** for menus and the palette: `is_visible`, `is_checked`, `description` (ask the host when a menu opens).
- [ ] **`Settings.add_on_change`** notifications (fire after `set_setting`/`persist_setting`/reload_settings).
- [ ] **Dialogs**: `ok_cancel_dialog`, `yes_no_cancel_dialog` (`rfd::MessageDialog` in a new DialogGateway method), `open_dialog`, `save_dialog`, `select_folder_dialog`.
- [ ] **Status and regions drawn**: `view.set_status` keys in the status bar; `add_regions` outlines/underlines/gutter icons in `editor_view.rs`.
- [ ] **Resources**: `sublime.find_resources(pattern)`, `load_resource`, `load_binary_resource` (ResourceGateway) — many plugins need them.
- [ ] `view.change_count()`, `view.is_scratch/set_scratch`, `view.close()`, `window.close_file`, `view.show/show_at_center` (scroll request to the view).

## Phase C — Package Control and full compatibility

Goal: install and run Package Control and the popular packages (LSP, GitGutter, …).

- [ ] **`.sublime-package` files** (zips) in `Installed Packages` and Sublime's install `Packages` folder: resources
      (syntaxes, keymaps, settings, menus) through the resource/syntax gateways, plugins through a zip importer in
      `plugin_host.py` (`zipimport`), and `load_resource` from zips. Loose folders override zipped files, as in Sublime.
- [ ] **`ignored_packages`**: skip those packages everywhere; Package Control toggles it while upgrading (needs `add_on_change`).
- [ ] **Libraries / dependencies**: ST4 `Lib/python38` (and Package Control's `dependencies.json` / `libraries`) on `sys.path`.
- [ ] **Python version**: plugins target 3.8 (or 3.3 via `.python-version`); we run whatever Python 3 is installed (3.14 here).
      Provide shims for removed modules (e.g. `imp`, gone in 3.12 — Sublime ships one for its 3.14 host) next to `sublime.py`.
- [ ] **Package Control bootstrap**: `install_package_control` downloads `Package Control.sublime-package` into
      `Installed Packages` (ask the user: it downloads and runs code). It uses urllib/ssl, threads, `set_timeout`,
      `view.set_status` (progress), `show_quick_panel`, `ok_cancel_dialog`, `run_command("open_url")`, `sublime.version()`.
- [ ] **Scopes** from the syntax engine (`syntax_engine.rs` / syntect parse state): `scope_name`, `match_selector`,
      `score_selector`, `extract_scope`, `style_for_scope`, `sublime.score_selector`, `view.meta_info`, `view.symbol_regions`.
- [ ] **Completions**: `on_query_completions`, `CompletionItem`, `view.auto_complete` (needs an auto-complete UI first).
- [ ] **Popups, phantoms, annotations, hover** (`show_popup` with minihtml, `PhantomSet`, `on_hover`).
- [ ] **Several windows, clones, groups/layouts** (`new_window`, `set_layout`, `move_to_group`).
- [ ] Reload only the plugin that was saved (now every plugin reloads), and `sublime_plugin.reload_plugin`.

Verification for phase C: install Package Control in a scratch profile (set `APPDATA` to a temp folder), install a
small package (e.g. a color scheme), then a plugin package, and run its commands.
