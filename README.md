# CodeLibre Text

CodeLibre Text - Un editor de código nativo, ligero y de código libre, construido con Rust y egui.

## Features

### Core
- **Multi-cursor editing** - Edit multiple positions simultaneously
- **Syntax highlighting** - Using tree-sitter for Rust, Python, JavaScript, TypeScript, C++, HTML, CSS, JSON
- **File management** - Open, save, save-as with proper encoding support
- **Undo/Redo** - Full undo/redo system with atomic operations
- **Tab bar** - Multiple file tabs with close, pin, and preview support
- **Split views** - Group support (planned)
- **Word wrap** - Toggle word wrapping
- **Minimap** - Code overview panel (toggleable)

### UI
- **Dark/Light themes** - Switch between dark and light color schemes
- **Command Palette** - Quick access to all commands (Ctrl+Shift+P)
- **Find & Replace** - With regex, case-sensitive, whole-word options
- **File sidebar** - Navigate project files
- **Status bar** - Shows cursor position, encoding, syntax, git branch
- **Menu bar** - File, Edit, Selection, View, Find, Tools, Help menus

### Git Integration
- **File status** - Shows git status indicators (modified, added, deleted, untracked)
- **Branch display** - Shows current git branch in status bar

### Plugin System
- **30+ Event handlers** - API de plugins con soporte asíncrono
  - `on_load`, `on_save`, `on_modified`, `on_selection_modified`, etc.
- **Completion API** - Auto-complete providers
- **Context queries** - Conditional menu/command logic
- **Async support** - Async event handlers

### Atajos de teclado (estilo editor moderno)
| Shortcut | Action |
|----------|--------|
| Ctrl+N | New File |
| Ctrl+O | Open File |
| Ctrl+S | Save File |
| Ctrl+W | Close File |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo |
| Ctrl+X | Cut |
| Ctrl+C | Copy |
| Ctrl+V | Paste |
| Ctrl+A | Select All |
| Ctrl+F | Find |
| Ctrl+H | Find & Replace |
| Ctrl+Shift+P | Command Palette |
| Ctrl+B | Toggle Sidebar |
| Ctrl+G | Goto Line |
| Ctrl+D | Find Under Expand |
| Ctrl+/ | Comment Line |
| Ctrl+= | Zoom In |
| Ctrl+- | Zoom Out |
| F11 | Fullscreen |

## Building

Requires Rust 2024 edition and Cargo.

```bash
cargo build --release
```

## Architecture

```
codelibre_text/
├── src/
│   ├── main.rs           # Entry point + CLI
│   ├── app.rs            # Main application state + rendering
│   ├── buffer.rs         # File buffer management
│   ├── text_buffer.rs    # Core text editing with multi-cursor
│   ├── undo.rs           # Undo/redo manager
│   ├── editor.rs         # Text editor widget (egui)
│   ├── syntax.rs         # tree-sitter syntax highlighting
│   ├── tab_bar.rs        # Tab management
│   ├── sidebar.rs        # File tree sidebar
│   ├── tree.rs           # Directory tree builder
│   ├── status_bar.rs     # Status bar display
│   ├── command_palette.rs# Command palette (fuzzy search)
│   ├── find_replace.rs   # Find & replace panel
│   ├── keybinding.rs     # Keyboard shortcut system
│   ├── menu.rs           # Menu bar system
│   ├── settings.rs       # User settings manager
│   ├── config.rs         # Configuration types
│   ├── theme.rs          # Color scheme definitions
│   ├── plugin.rs         # Plugin API
│   ├── git.rs            # Git status provider
│   ├── project.rs        # Project management
│   └── cli.rs            # Command line interface
├── Cargo.toml
├── README.md
└── docs/
    └── architecture.md   # Detailed architecture docs
```

## Comparación con editores modernos

| Característica | Sublime Text | CodeLibre Text |
|---------------|-------------|-------------|
| Lenguaje | C++ (core) + Python (plugins) | Rust (todo) |
| UI Framework | Native C++ | egui (modo inmediato) |
| Tamaño | ~8 MB | ~3-5 MB |
| Inicio | Instant | Instant |
| Memoria | Baja | Más baja |
| Sintaxis | .sublime-syntax | tree-sitter |
| Plugins | Python | Rust traits + WASM |
| Multi-cursor | Sí | Sí |
| Paleta de comandos | Sí | Sí |
| Buscar/Reemplazar | Sí | Sí |
| Git | Vía plugins | Integrado |
| Multi-plataforma | Windows/macOS/Linux | Windows/macOS/Linux |

## Roadmap

- [x] Core text editing
- [x] Tab management
- [x] Syntax highlighting
- [x] Find & Replace
- [x] Command Palette
- [x] File sidebar
- [x] Status bar
- [x] Git integration
- [x] Plugin API
- [ ] Split views / groups
- [ ] LSP integration
- [ ] Auto-complete / IntelliSense
- [ ] Multi-select (Ctrl+D)
- [ ] Goto File (quick open)
- [ ] Macro recording
- [ ] Build systems
- [ ] Package manager
- [ ] Extension marketplace

## License

MIT
