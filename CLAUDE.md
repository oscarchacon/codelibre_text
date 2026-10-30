# CLAUDE.md

Native desktop text editor compatible with Sublime Text resources, written with **Clean Architecture**
(Robert C. Martin's circles — not DDD, not hexagonal/ports-and-adapters vocabulary). Rust workspace,
edition 2021, egui/eframe 0.31, syntect 5 (`default-fancy`). Read `docs/architecture.md` first.

## Workspace = the four circles (dependencies point inwards only)
- `crates/entities` — business rules, no I/O (`Document`, `Selection`, `text_commands`, `find`, `build`, `Keymap`, `Settings`, `Menu`).
- `crates/use_cases` — `Editor` + use case modules + gateway traits (`gateways.rs`).
- `crates/interface_adapters` — controllers, presenters, view models, `ViewState`, Sublime file-format gateways.
- `crates/frameworks` — package `codelibre_text` = library (`lib.rs`: `compose()` composition root, egui views, `app.rs` loop,
  syntect engine, OS/library gateways) + thin `main.rs` (CLI + window).
- `crates/test_support` — test doubles only (dev-dependency): `World` implements every gateway in memory.
- `assets/Default/` — built-in keymap, menu, palette, preferences, color schemes, and `Localization/` (`messages.pot` + one
  gettext `.po` per interface language, parsed by `gateways/po_file.rs`) (`include_str!` in `interface_adapters/src/gateways/builtin_resources.rs`).
- `assets/python/` — the Python plugin host and our own `sublime` / `sublime_plugin` modules (`include_str!` in
  `frameworks/src/gateways/python_plugin_host.rs`). Design, protocol and the **pending phases B and C**: `docs/plugins.md`.
- `assets/icons/` — app icon (`.png` window icon in `main.rs`, `.ico` embedded in the .exe by `frameworks/build.rs`, `.icns` for macOS);
  `packaging/linux` (`.desktop` + `install.sh`) and `packaging/macos` (`Info.plist` + `make_app.sh`). `.gitattributes` keeps them LF.

Never add an outer-circle dependency to an inner crate's `Cargo.toml`; use a gateway trait instead.
Keep Clean Architecture names: Entities, Use Cases, Controllers, Presenters, View Models, Gateways.

## Build / verify
- `cargo build` / `cargo run -- <folder-or-file>...` (root `default-members` is the frameworks crate).
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, plus a manual GUI run for views.
- Tests live in each crate's `tests/` folder, one file per tested module (no `#[cfg(test)]` in `src/`).
  Use `test_support::World` for use case / adapter tests and `tempfile` for real file-system tests.
  `use_cases/tests/documents.rs` sleeps 1.6 s (external-change check interval).
  `frameworks/tests/python_plugin_host.rs` runs a real Python 3 (skipped with a message when none is found).
- `Cargo.lock` is gitignored. Dependencies build at `opt-level = 2` in dev (syntect is slow unoptimized).
- Only positional `files` and `--fullscreen` are used from `crates/frameworks/src/cli.rs`.

## Adding things
- Text users see: never a bare literal. Use `tr` / `tr_args` / `tr_n` (on `Editor`, or `Translations` in views) with a
  string literal, then `UPDATE_TRANSLATIONS=1 cargo test -p interface_adapters --test builtin_resources` and translate
  the new entries in every `assets/Default/Localization/*.po` (that test fails otherwise). See `docs/translating.md`.
- Sublime command: arm in `interface_adapters/src/controllers/command_controller.rs` → use case → expose in `assets/Default/*`.
- Text transformation: `entities/src/text_commands.rs` + `TextCommand` variant in `use_cases/src/editing.rs`.
- External system: trait in `use_cases/src/gateways.rs`, impl in `frameworks/src/gateways/`, wire in `compose()`
  (`frameworks/src/lib.rs`), and add a fake to `crates/test_support`.
- Plugin API function: method in `assets/python/sublime.py` → arm in `use_cases/src/plugin_api.rs` → test in
  `use_cases/tests/plugin_api.rs` (see `docs/plugins.md`). Command names without a controller arm go to the plugins.
- Views draw view models and push `UiEvent`s; they must not mutate state (exception: the text area, see below).

## Gotchas
- The editor view edits `Document::text` in place via `Editor::text_for_editing` (egui `TextEdit` needs `&mut String`).
  Command-made selections reach the widget through `Document::selection_revision`; the view reports carets with `UiEvent::SelectionChanged`.
- Undo/copy/cut/paste/select all are `ViewRequest::WidgetEdit`s replayed as egui input next frame; key bindings to
  them pass through (`WIDGET_COMMANDS` in `keyboard_controller.rs`). egui sends Ctrl+C/X/V as `Event::Copy/Cut/Paste`
  (no key event): `app.rs::dispatch_keys` asks `keyboard_controller::clipboard_shortcut`, which takes them over only for
  whole-line copy/paste (`copy_with_empty_selection`, needs `ViewState::text_has_focus`).
- Selections are char indices; edit with byte offsets (`entities::text::char_to_byte` / `byte_to_char`).
- Chrome widgets must use `Sense::CLICK`, not `Sense::click()` (focusable widgets steal arrow keys/Enter).
- `app.rs::dispatch_keys` drops the `Text` event after a consumed key (Alt+1 would type "1").
- Multiple selections (Ctrl+D, Alt+F3 `find_all_under`, Ctrl+Alt+Up/Down `select_lines`, Ctrl+Shift+L
  `split_selection_into_lines`; commands set them with `Document::set_selections`): the widget only knows the primary one (`Document::selection`); `extra_selections` are painted
  by `editor_view.rs`, whose typing/delete/caret keys become `UiEvent::{TypedAtSelections, DeleteAtSelections, MoveCarets}`
  → `Document::edit_all`. Any other caret move (click, Up/Down, a command's `set_selection`) drops the extras.
- Default fonts lack ▸ ▾ ● ✕; draw shapes or use `×`.
- `persist_setting` re-serializes the user's `Preferences.sublime-settings` (comments are lost), as Sublime does.
- Plugin commands and events run synchronously on the UI thread (20 s timeout kills the host); timers and plugin
  threads call the editor between frames (`Editor::poll_plugins`, every frame in `app.rs`). Python code must never write to the
  host's real stdout (it is the protocol); `codelibre_rpc.setup()` redirects `print` to the console.
- Every `command_controller::run` asks the plugins' `on_text_command` / `on_window_command` (when some listener has them)
  and tells `on_post_*_command` after (a widget edit: once the widget did it, a frame later); those listeners also turn undo / redo / select all / clipboard keys into commands; with the fake host, script one `PluginMessage::Answer` per query.
- Python `Region` has `__len__`, so a caret is falsy: test `is None`, never `if region:`.

## Runtime state (Windows)
- Packages: `%APPDATA%\codelibre_text\data\Packages` (`User` inside); Sublime's `%APPDATA%\Sublime Text\Packages`
  is also read unless `import_sublime_packages` is false (its `Default` package is skipped).
- `.sublime-package` zips (ours in `data\Installed Packages`, Sublime's installed and shipped ones) are unpacked to
  `data\Cache\Packages`, then read as plain folders (`package_archives.rs`, `PackagePaths::roots`); loose files win. Of what
  Sublime ships, syntaxes are left out. `ignored_packages` defaults to `Vintage`.
- Session (hot exit): `%APPDATA%\codelibre_text\data\Local\Session.sublime_session`, written on window close.
- `CODELIBRE_TEXT_DATA=<folder>` uses that folder instead of all of the above, to try things without touching the
  real profile (setting `APPDATA` does not work: Windows answers the known folder).
- Plugin host modules are rewritten at every start to `%APPDATA%\codelibre_text\data\Lib\python`.

## Pending work (read before starting new features)
- `docs/plugins.md` — Python plugins: phase A done; phase B core done (call ids, timers, view events, quick / input panels,
  dialogs, resources, scratch views, output panels and their results, build systems, the input panel's view, statuses and regions drawn, query_context, command hooks, input handlers, command metadata) — the rest
  of B (what each item lists as still missing) and C (Package Control,
  `.sublime-package`, scopes) are checklists with the files to touch.
- `docs/sublime-gap-analysis.md` — every Sublime command / setting not implemented yet, by feature.
- `docs/sublime-native-commands.md` — Sublime's native commands (from its binary, via Ghidra) with the arguments and
  settings each reads; check it before implementing a command so its arguments match Sublime's.

## Not implemented
Multiple cursors via mouse (no Ctrl+click or column selection), Up/Down or copy of all selections, the rest of plugin phase B and C
(`docs/plugins.md`), split groups, minimap, folding, auto-complete, find in files.
Highlighting is skipped above 4 MB.
