# Architecture - CodeLibre Text

## Design Decisions

### Why Rust + egui?

Sublime Text is written in C++ with its own native UI engine. We replicate this with:

- **Rust**: Memory safety + performance, no GC pause
- **egui**: Immediate mode GUI, no browser, no Chromium, no Electron
- **tree-sitter**: Better parsing than Sublime's .sublime-syntax files
- **git2**: Native git integration, no subprocess

### Architecture Layers

```
┌─────────────────────────────────────────────┐
│  Presentation Layer (egui)                  │
│  - Title bar, Menu bar                      │
│  - Tab bar                                  │
│  - Sidebar (file tree)                      │
│  - Editor view                              │
│  - Status bar                               │
│  - Panels (Command Palette, Find/Replace)   │
├─────────────────────────────────────────────┤
│  Application Layer (app.rs)                 │
│  - State management                         │
│  - File operations                          │
│  - Event handling                           │
│  - Plugin system                            │
├─────────────────────────────────────────────┤
│  Editor Layer (editor.rs, text_buffer.rs)   │
│  - TextBuffer: Core text model              │
│  - Cursor management (multi-cursor)         │
│  - Region management                      │
│  - Syntax highlighting rendering            │
├─────────────────────────────────────────────┤
│  Buffer Layer (buffer.rs)                   │
│  - File state tracking                      │
│  - Syntax detection                         │
│  - Tab mapping                              │
├─────────────────────────────────────────────┤
│  Services (git.rs, settings.rs, etc.)       │
│  - Git status                               │
│  - Settings persistence                     │
│  - Keybinding resolution                    │
│  - Menu command dispatch                    │
└─────────────────────────────────────────────┘
```

## Module Reference

### core/

| Module | Purpose |
|--------|---------|
| `text_buffer.rs` | Core text editing - insert, delete, cursor management, multi-cursor, regions |
| `buffer.rs` | File buffer - wraps TextBuffer with file state, syntax, settings |
| `undo.rs` | Undo/redo - Command pattern with atomic group operations |
| `syntax.rs` | Syntax highlighting using tree-sitter parser |
| `editor.rs` | egui editor widget - renders text, highlights syntax, handles input |

### ui/

| Module | Purpose |
|--------|---------|
| `app.rs` | Main application - state, rendering, event loop |
| `tab_bar.rs` | Tab management - add/remove/close/activate tabs |
| `sidebar.rs` | File sidebar - tree view, file selection |
| `status_bar.rs` | Status bar - cursor pos, encoding, syntax, git info |
| `theme.rs` | Color schemes - dark and light themes |
| `command_palette.rs` | Command palette - fuzzy search for commands |
| `find_replace.rs` | Find/Replace panel - regex, case-sensitive |
| `menu.rs` | Menu bar - file/edit/view menus |

### services/

| Module | Purpose |
|--------|---------|
| `settings.rs` | User settings - load/save JSON settings |
| `keybinding.rs` | Keyboard shortcuts - Sublime Text keybindings |
| `git.rs` | Git integration - file status via git2 |
| `project.rs` | Project management - multi-root projects |
| `plugin.rs` | Plugin system - event handlers, completions |

## Data Flow

```
User Input → KeybindingManager → Command Palette → MenuManager
                                        ↓
                                   EditorView
                                        ↓
                                   TextBuffer
                                        ↓
                                UndoManager ←→ Editor Render
```

## Key Types

### TextBuffer (text_buffer.rs)
- The core text model
- Manages: content, cursors, regions, changes
- Operations: insert(), delete(), replace(), find_all()

### Buffer (buffer.rs)
- Wraps TextBuffer with file metadata
- Manages: file_path, state, syntax, zoom, word_wrap

### EditorView (editor.rs)
- egui widget for rendering
- Manages: scroll, zoom, committed text, IME

### Tab (tab_bar.rs)
- Maps to a Buffer via buffer_id
- Manages: display name, modified indicator, preview

### UndoManager (undo.rs)
- Command pattern for undo/redo
- Supports atomic grouping of changes

## Paralelos con editores modernos

| Editor moderno | CódigoLibre |
|---------------|-------------|
| Sheet | Buffer + TextBuffer |
| View | EditorView |
| Window | CodeLibreTextApp |
| .sublime-syntax | tree-sitter grammar |
| .sublime-keymap | keybinding.rs |
| .sublime-menu | menu.rs |
| Plugin API | plugin.rs |
| Packages/ | Project folders |
