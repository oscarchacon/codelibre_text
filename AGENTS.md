# AGENTS.md

Native desktop text editor compatible with Sublime Text resources, written with **Clean Architecture**
(Robert C. Martin's circles — not DDD, not hexagonal/ports-and-adapters vocabulary). Rust workspace,
edition 2021, egui/eframe 0.31, syntect 5 (`default-fancy`). Read `docs/architecture.md` first.

## Workspace = the four circles (dependencies point inwards only)
- `crates/entities` — business rules, no I/O (`Document`, `Selection`, `text_commands`, `find`, `Keymap`, `Settings`, `Menu`).
- `crates/use_cases` — `Editor` + use case modules + gateway traits (`gateways.rs`).
- `crates/interface_adapters` — controllers, presenters, view models, `ViewState`, Sublime file-format gateways.
- `crates/frameworks` — package `codelibre_text` = library (`lib.rs`: `compose()` composition root, egui views, `app.rs` loop,
  syntect engine, OS/library gateways) + thin `main.rs` (CLI + window).
- `crates/test_support` — test doubles only (dev-dependency): `World` implements every gateway in memory.
- `assets/Default/` — built-in keymap, menu, palette, preferences, color schemes (`include_str!` in `interface_adapters/src/gateways/builtin_resources.rs`).
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
- `Cargo.lock` is gitignored. Dependencies build at `opt-level = 2` in dev (syntect is slow unoptimized).
- Only positional `files` and `--fullscreen` are used from `crates/frameworks/src/cli.rs`.

## Adding things
- Sublime command: arm in `interface_adapters/src/controllers/command_controller.rs` → use case → expose in `assets/Default/*`.
- Text transformation: `entities/src/text_commands.rs` + `TextCommand` variant in `use_cases/src/editing.rs`.
- External system: trait in `use_cases/src/gateways.rs`, impl in `frameworks/src/gateways/`, wire in `compose()`
  (`frameworks/src/lib.rs`), and add a fake to `crates/test_support`.
- Views draw view models and push `UiEvent`s; they must not mutate state (exception: the text area, see below).

## Gotchas
- The editor view edits `Document::text` in place via `Editor::text_for_editing` (egui `TextEdit` needs `&mut String`).
  Command-made selections reach the widget through `Document::selection_revision`; the view reports carets with `UiEvent::SelectionChanged`.
- Undo/copy/cut/paste/select all are `ViewRequest::WidgetEdit`s replayed as egui input next frame; key bindings to
  them pass through (`WIDGET_COMMANDS` in `keyboard_controller.rs`).
- Selections are char indices; edit with byte offsets (`entities::text::char_to_byte` / `byte_to_char`).
- Chrome widgets must use `Sense::CLICK`, not `Sense::click()` (focusable widgets steal arrow keys/Enter).
- `app.rs::dispatch_keys` drops the `Text` event after a consumed key (Alt+1 would type "1").
- Multiple selections (Ctrl+D): the widget only knows the primary one (`Document::selection`); `extra_selections` are painted
  by `editor_view.rs`, whose typing/delete/caret keys become `UiEvent::{TypedAtSelections, DeleteAtSelections, MoveCarets}`
  → `Document::edit_all`. Any other caret move (click, Up/Down, a command's `set_selection`) drops the extras.
- Default fonts lack ▸ ▾ ● ✕; draw shapes or use `×`.
- `persist_setting` re-serializes the user's `Preferences.sublime-settings` (comments are lost), as Sublime does.

## Runtime state (Windows)
- Packages: `%APPDATA%\codelibre_text\data\Packages` (`User` inside); Sublime's `%APPDATA%\Sublime Text\Packages`
  is also read unless `import_sublime_packages` is false (its `Default` package is skipped).
- Session (hot exit): `%APPDATA%\codelibre_text\data\Local\Session.sublime_session`, written on window close.

## Not implemented
Multiple cursors other than Ctrl+D (no Ctrl+click, column selection, Up/Down or copy of all selections), Python plugins, `.sublime-package` zips,
split groups, minimap, folding, auto-complete, find in files. Highlighting is skipped above 4 MB.
