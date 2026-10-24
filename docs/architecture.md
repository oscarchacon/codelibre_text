# Architecture

CodeLibre Text follows **Clean Architecture** (Robert C. Martin). The code is split into four
concentric circles, and **source code dependencies only point inwards**: an inner circle never
names anything from an outer one.

```
┌──────────────────────────────────────────────────────────────────────┐
│ Frameworks & Drivers      crates/frameworks   (binary: codelibre_text)│
│   egui views · syntect · rfd dialogs · clipboard · disk · main()      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │ Interface Adapters    crates/interface_adapters                 │  │
│  │   controllers · presenters · view models · Sublime file formats  │  │
│  │  ┌──────────────────────────────────────────────────────────┐  │  │
│  │  │ Use Cases         crates/use_cases                         │  │  │
│  │  │   Editor (application state) · use cases · gateway traits  │  │  │
│  │  │  ┌────────────────────────────────────────────────────┐  │  │  │
│  │  │  │ Entities      crates/entities                       │  │  │  │
│  │  │  │   Document · Selection · text commands · find ·     │  │  │  │
│  │  │  │   Keymap · Settings · Menu · Command                │  │  │  │
│  │  │  └────────────────────────────────────────────────────┘  │  │  │
│  │  └──────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
                    dependencies point inwards  ──►
```

## The Dependency Rule is enforced by the compiler

Each circle is its own crate, and each crate's `Cargo.toml` only lists the crates inside it:

| Crate | May depend on | Third-party crates |
|---|---|---|
| `entities` | nothing | `serde_json` (values), `regex` (find) |
| `use_cases` | `entities` | `serde_json`, `fuzzy-matcher` |
| `interface_adapters` | `use_cases`, `entities` | `serde`, `serde_json` |
| `frameworks` | all of the above | `egui`, `eframe`, `syntect`, `rfd`, `arboard`, `directories`, `ignore`, `clap` |

If someone writes `use egui` inside `use_cases`, the build fails. `cargo tree -p use_cases` shows
exactly what a circle can see.

## The circles

### 1. Entities — `crates/entities`
The editor's business rules, true for any editor regardless of how it is shown:

| Module | What it is |
|---|---|
| `document.rs` | An open buffer: text, path, encoding, line endings, dirty state, selection. Decodes/encodes file bytes. |
| `selection.rs`, `text.rs` | Selections (char indices) and position helpers. |
| `text_commands.rs` | Pure text transformations: duplicate line, toggle comment, indent, sort lines… |
| `find.rs` | Find / replace with regex, case, whole word and wrap. |
| `keymap.rs` | Key combos in Sublime notation, chords, context conditions. |
| `settings.rs` | Layered settings (defaults → packages → User → syntax → runtime). |
| `menu.rs`, `command.rs`, `symbol.rs` | Menus, palette entries, command invocations, Goto Symbol entries. |

No I/O, no UI. Everything here is unit-tested.

### 2. Use Cases — `crates/use_cases`
What the application does. `Editor` holds the application state (`EditorState`) and the gateways;
each module adds the use cases of one area as methods:

| Module | Use cases |
|---|---|
| `documents.rs` | new, open, switch tabs, close (asking about unsaved changes), reopen, revert, reload changed files |
| `saving.rs` | save, save as, save all |
| `editing.rs` | `run_text_command(TextCommand)`, go to line, clipboard text |
| `find_replace.rs` | find next/previous, find under caret, replace, cached matches |
| `preferences.rs` | settings, font size, color scheme, edit settings files |
| `packages.rs` | load and reload Sublime resources |
| `project.rs` | folders, folder listing, file index |
| `search.rs` | Command Palette, Goto Anything (`file`, `:line`, `@symbol`), color scheme picker |
| `session.rs` | hot exit: save/restore the session, exit confirmation |

**Gateways** (`gateways.rs`) are the traits through which use cases reach the outside world:
`FileSystemGateway`, `DialogGateway`, `ClipboardGateway`, `ResourceGateway`, `SyntaxGateway`,
`SessionGateway`, `FileIndexGateway`, `ShellGateway`. They are *declared* here and *implemented*
in the outer circles — that is how an inner circle uses something outer without depending on it.

Use cases are tested with in-memory gateways (`crates/use_cases/tests/document_workflow.rs`).

### 3. Interface Adapters — `crates/interface_adapters`
Converts data between the use cases and the outside:

| Folder | Role |
|---|---|
| `controllers/` | Input → use cases. `command_controller` maps Sublime command names (`save`, `show_overlay`…) to use cases; `keyboard_controller` resolves keys through the keymap; `ui_controller` handles `UiEvent`s from the views. |
| `presenters/` | Application state → view models, one presenter per part of the window. |
| `view_models.rs` | Plain data the views draw (`TabViewModel`, `EditorViewModel`…). |
| `view_state.rs` | Presentation-only state: open panels, expanded folders, queued commands. |
| `gateways/` | Gateway implementations about **file formats**: Sublime's JSON dialect, `.sublime-settings/-keymap/-menu/-commands`, `.sublime-color-scheme`, the session file. |

### 4. Frameworks & Drivers — `crates/frameworks`
Everything tied to a library or the OS:

| Path | Role |
|---|---|
| `main.rs` | Composition root: builds every gateway implementation and starts the `Editor`. |
| `app.rs` | The eframe loop (see below). |
| `views/` | egui views. They draw view models and return `UiEvent`s. |
| `input.rs` | egui keys → `KeyCombo`. |
| `syntax_engine.rs` | syntect: syntaxes, color schemes, incremental per-line highlighting. |
| `gateways/` | Disk (`std_file_system`), `rfd` dialogs, `arboard` clipboard, syntect `SyntaxGateway`, `ignore` file index, OS shell, platform directories. |

## How one frame works

```
 egui input ──► keyboard_controller ──► Keymap (entity) ──► queued Command
                                                               │
 queued Commands ──► command_controller ──► Editor use cases ──┤ (state changes)
                                                               │
 Editor state ──► presenters ──► view models ──► egui views ───┘
                                                    │
                                   UiEvents ◄───────┘ ──► ui_controller ──► use cases
```

`app.rs::update` runs those steps in order: dispatch keys → run queued commands → carry out view
requests (fullscreen, close, widget edits) → present + draw → handle `UiEvent`s.

### Example: pressing Ctrl+S

1. `frameworks/app.rs` turns the egui key into `KeyCombo { ctrl, key: "s" }` (`input.rs`).
2. `keyboard_controller` asks the `Keymap` entity, which finds `{ "keys": ["ctrl+s"], "command": "save" }`.
3. `command_controller::run` maps `save` to `Editor::save_active` (use case).
4. `save_active` encodes the `Document` entity and writes it through `FileSystemGateway`
   (implemented by `StdFileSystem` in frameworks).
5. Next frame, the tabs presenter no longer marks the tab dirty and the view draws it.

## Deliberate, documented exceptions

- **Typing edits the text in place.** egui's `TextEdit` needs `&mut String` and owns typing, caret
  movement, IME and undo; copying the whole buffer every frame would be too slow. So the editor view
  gets the text through `Editor::text_for_editing` and reports the caret back with
  `UiEvent::SelectionChanged`. Selections made by commands flow the other way: when
  `Document::selection_revision` changes, the view moves the widget's caret.
- **Undo, copy, cut, paste and select all** belong to the text widget, so the controllers turn
  those commands into `ViewRequest::WidgetEdit`, which `app.rs` replays as widget input.
- **`serde_json` and `regex` in Entities.** They are data utilities, not frameworks: settings and
  command arguments are JSON values in Sublime, and find is defined by regular expressions.
- **File-format gateways read files with `std::fs`.** The standard library is not a framework;
  those gateways exist to understand Sublime's formats.

## Where to put a change

| I want to… | Go to |
|---|---|
| Add a text transformation | `entities/src/text_commands.rs` (+ a `TextCommand` variant in `use_cases/src/editing.rs`) |
| Add a Sublime command | an arm in `interface_adapters/src/controllers/command_controller.rs`, then the keymap/menu/palette in `assets/Default/` |
| Add application behavior | a method in the matching `use_cases/src/*.rs` module |
| Talk to a new external system | a trait in `use_cases/src/gateways.rs`, its implementation in `frameworks/src/gateways/`, wired in `frameworks/src/main.rs` |
| Support another Sublime file format | `interface_adapters/src/gateways/` |
| Change how something looks | a presenter (what is shown) or a view (how it is drawn) |

## Tests

`cargo test --workspace` runs the entity unit tests, the format-gateway tests and the use case
tests with in-memory gateways. The egui layer is verified by running the app.
