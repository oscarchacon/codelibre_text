# CodeLibre Text

CodeLibre Text - A lightweight, native, open-source code editor built with Rust and egui,
compatible with Sublime Text's resource formats.

## Features

### Editing
- Real text editing with undo/redo, clipboard, mouse selection and IME (egui `TextEdit`)
- Syntax highlighting with [syntect](https://github.com/trishume/syntect) (Sublime's own
  `.sublime-syntax` engine), re-highlighted incrementally per line
- Auto indent, Tab/Shift+Tab indent, `translate_tabs_to_spaces`, rulers, word wrap
- Line commands: duplicate, swap up/down, delete, join, add line before/after, toggle comment,
  case conversion, sort/unique/reverse lines, trim trailing whitespace, tabs <-> spaces
- Find & replace panel: regex, case sensitive, whole word, wrap, match highlighting
- Files: UTF-8 (with/without BOM) and Latin-1, CRLF/LF preserved, reload on external changes
- Hot exit: open folders and unsaved buffers are restored on the next start

### UI (Sublime-style)
- Menu bar, tabs, side bar with OPEN FILES and a lazily loaded FOLDERS tree
- Command Palette (`Ctrl+Shift+P`), Goto Anything (`Ctrl+P`, with `:line` and `@symbol`)
- Status bar: caret position, indentation, line endings, encoding, syntax picker
- Window chrome colors adapt to the active color scheme

### Sublime Text compatibility
Resources are loaded from `Packages/<Package>/` in Sublime's order (`Default`, other packages,
`User` last). Packages live in `%APPDATA%\codelibre_text\data\Packages` (Windows); if Sublime Text
is installed, its `Packages` folder is read as well (`"import_sublime_packages": false` disables it).

| Resource | Support |
|----------|---------|
| `.sublime-syntax` | Yes (via syntect; some Sublime Text 4-only features may fail, see Help > Package Load Errors) |
| `.tmTheme`, `.sublime-color-scheme` | Yes (`variables`, `globals`, `rules`, `color(... alpha())`) |
| `.sublime-settings` | Yes, layered: defaults -> packages -> `User`, plus per-syntax files |
| `.sublime-keymap` | Yes: chords, args, and `overlay_visible`/`panel_visible`/`setting.*`/`selection_empty` contexts |
| `Main.sublime-menu` | Yes, merged by `id` |
| `.sublime-commands` | Yes |
| Sublime command names | Built-in subset (`save`, `show_overlay`, `toggle_comment`, `run_macro_file`, …) |
| Python plugins (`.py`) | Not yet: plugin commands show "Command not available" |
| `.sublime-package` archives | Not yet: only unpacked package folders are read |

### Keyboard shortcuts (Sublime Text defaults)
| Shortcut | Action |
|----------|--------|
| Ctrl+N / Ctrl+O / Ctrl+K, Ctrl+O | New file / Open file / Open folder |
| Ctrl+S / Ctrl+Shift+S | Save / Save as |
| Ctrl+W / Ctrl+Shift+T | Close / Reopen closed file |
| Ctrl+Shift+P | Command Palette |
| Ctrl+P / Ctrl+G / Ctrl+R | Goto Anything / Line / Symbol |
| Ctrl+F / Ctrl+H / F3 / Shift+F3 | Find / Replace / Next / Previous |
| Ctrl+D / Ctrl+F3 | Select word, then next occurrence / Quick find |
| Ctrl+/ / Ctrl+Shift+/ | Toggle line / block comment |
| Ctrl+Shift+D / Ctrl+Shift+K | Duplicate / Delete line |
| Ctrl+Shift+Up / Down | Swap line up / down |
| Ctrl+L / Ctrl+J | Expand selection to line / Join lines |
| Ctrl+] / Ctrl+[ | Indent / Unindent |
| Ctrl+K, Ctrl+U / Ctrl+K, Ctrl+L | Upper / lower case |
| Ctrl+K, Ctrl+B | Toggle side bar |
| Alt+1..9, Ctrl+PageUp/PageDown, Ctrl+Tab | Switch tabs |
| Ctrl+= / Ctrl+- / Ctrl+0 | Font size |
| F11 | Full screen |

Override them in Preferences > Key Bindings, exactly like in Sublime Text.

## Building

Requires a recent stable Rust toolchain (edition 2021).

```bash
cargo build --release
```

```bash
cargo run -- path/to/folder path/to/file.rs
```

```bash
cargo test --workspace
```

## Architecture

The project follows **Clean Architecture**. Each circle is a crate of the Cargo workspace, so the
compiler enforces that dependencies only point inwards:

```
crates/
├── entities/            # Entities: Document, Selection, text commands, find, keymap, settings, menus
├── use_cases/           # Use Cases: Editor + use cases (open, save, find…) + gateway traits
├── interface_adapters/  # Controllers, presenters, view models, Sublime file-format gateways
├── frameworks/          # Frameworks & Drivers: egui views, syntect, dialogs, disk, main() (binary)
└── test_support/        # Test doubles: in-memory gateways (tests only)
assets/Default/          # Built-in keymap, menu, commands, preferences and color schemes
```

Each crate keeps its tests in `crates/<crate>/tests/`, one file per module it tests.

See [docs/architecture.md](docs/architecture.md) for the circles, the flow of a frame, an
end-to-end example (pressing Ctrl+S), where each kind of change belongs, and how the tests are
organized.

## Roadmap

- [ ] Multiple cursors / multiple selections
- [ ] Python plugin host implementing the `sublime` / `sublime_plugin` API
- [ ] `.sublime-package` (zip) loading and Package Control
- [ ] Split layouts (groups)
- [ ] Minimap, code folding, auto-complete
- [ ] Find in Files, build systems
- [ ] Faster layout for very large files

## License

This project is licensed under the [MIT License](LICENSE).
