# CLAUDE.md

Native desktop text editor compatible with Sublime Text resources, written with **Clean Architecture**
(Robert C. Martin's circles — not DDD, not hexagonal/ports-and-adapters vocabulary). Rust workspace,
edition 2021, egui/eframe 0.31, syntect 5 (`default-fancy`). Read `docs/architecture.md` first.

## Workspace = the four circles (dependencies point inwards only)
- `crates/entities` — business rules, no I/O (`Document`, `Selection`, `text_commands`, `find`, `Keymap`, `Settings`, `Menu`).
- `crates/use_cases` — `Editor` + use case modules + gateway traits (`gateways.rs`).
- `crates/interface_adapters` — controllers, presenters, view models, `ViewState`, Sublime file-format gateways.
- `crates/frameworks` — binary `codelibre_text`: egui views, `app.rs` loop, syntect engine, OS/library gateways, `main.rs` composition root.
- `assets/Default/` — built-in keymap, menu, palette, preferences, color schemes (`include_str!` in `interface_adapters/src/gateways/builtin_resources.rs`).

Never add an outer-circle dependency to an inner crate's `Cargo.toml`; use a gateway trait instead.
Keep Clean Architecture names: Entities, Use Cases, Controllers, Presenters, View Models, Gateways.

## Build / verify
- `cargo build` / `cargo run -- <folder-or-file>...` (root `default-members` is the frameworks crate).
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, plus a manual GUI run for views.
- `Cargo.lock` is gitignored. Dependencies build at `opt-level = 2` in dev (syntect is slow unoptimized).
- Only positional `files` and `--fullscreen` are used from `crates/frameworks/src/cli.rs`.

## Adding things
- Sublime command: arm in `interface_adapters/src/controllers/command_controller.rs` → use case → expose in `assets/Default/*`.
- Text transformation: `entities/src/text_commands.rs` + `TextCommand` variant in `use_cases/src/editing.rs`.
- External system: trait in `use_cases/src/gateways.rs`, impl in `frameworks/src/gateways/`, wire in `frameworks/src/main.rs`.
- Views draw view models and push `UiEvent`s; they must not mutate state (exception: the text area, see below).

## Gotchas
- The editor view edits `Document::text` in place via `Editor::text_for_editing` (egui `TextEdit` needs `&mut String`).
  Command-made selections reach the widget through `Document::selection_revision`; the view reports carets with `UiEvent::SelectionChanged`.
- Undo/copy/cut/paste/select all are `ViewRequest::WidgetEdit`s replayed as egui input next frame; key bindings to
  them pass through (`WIDGET_COMMANDS` in `keyboard_controller.rs`).
- Selections are char indices; edit with byte offsets (`entities::text::char_to_byte` / `byte_to_char`).
- Chrome widgets must use `Sense::CLICK`, not `Sense::click()` (focusable widgets steal arrow keys/Enter).
- `app.rs::dispatch_keys` drops the `Text` event after a consumed key (Alt+1 would type "1").
- Default fonts lack ▸ ▾ ● ✕; draw shapes or use `×`.
- `persist_setting` re-serializes the user's `Preferences.sublime-settings` (comments are lost), as Sublime does.

## Runtime state (Windows)
- Packages: `%APPDATA%\codelibre_text\data\Packages` (`User` inside); Sublime's `%APPDATA%\Sublime Text\Packages`
  is also read unless `import_sublime_packages` is false (its `Default` package is skipped).
- Session (hot exit): `%APPDATA%\codelibre_text\data\Local\Session.sublime_session`, written on window close.

## Not implemented
Multiple cursors (`find_under_expand` moves a single selection), Python plugins, `.sublime-package` zips,
split groups, minimap, folding, auto-complete, find in files. Highlighting is skipped above 4 MB.
