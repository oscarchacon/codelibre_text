//! Keybindings - Custom keyboard shortcut system

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombination {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

impl KeyCombination {
    pub fn new(ctrl: bool, alt: bool, shift: bool, key: String) -> Self {
        Self { ctrl, alt, shift, key }
    }

    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl"); }
        if self.alt { parts.push("Alt"); }
        if self.shift { parts.push("Shift"); }
        parts.push(&self.key);
        parts.join("+")
    }
}

/// A single keybinding entry
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub keys: Vec<KeyCombination>,
    pub command: String,
    pub description: String,
}

/// A boxed command handler
pub struct CommandHandler(pub Box<dyn Fn() + Send + Sync>);

impl std::fmt::Debug for CommandHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandHandler").finish()
    }
}

/// Keybinding manager
#[derive(Debug)]
pub struct KeybindingManager {
    bindings: Vec<KeyBinding>,
    commands: HashMap<String, CommandHandler>,
}

impl KeybindingManager {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            commands: HashMap::new(),
        }
    }

    pub fn add_binding(&mut self, binding: KeyBinding) {
        self.bindings.push(binding);
    }

    pub fn find_command(&self, keys: &KeyCombination) -> Option<&str> {
        self.bindings.iter()
            .find(|b| b.keys.contains(keys))
            .map(|b| b.command.as_str())
    }

    pub fn register_command(&mut self, name: &str, handler: CommandHandler) {
        self.commands.insert(name.to_string(), handler);
    }

    pub fn execute_command(&self, command: &str) -> Option<&CommandHandler> {
        self.commands.get(command)
    }

    /// Load default keybindings (modern editor style)
    pub fn load_defaults(&mut self) {
        // File operations
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "N".to_string())],
            command: "new_file".to_string(),
            description: "New File".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "O".to_string())],
            command: "open_file".to_string(),
            description: "Open File".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "S".to_string())],
            command: "save_file".to_string(),
            description: "Save File".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, true, false, "S".to_string())],
            command: "save_all".to_string(),
            description: "Save All".to_string(),
        });

        // Edit operations
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "Z".to_string())],
            command: "undo".to_string(),
            description: "Undo".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "Y".to_string())],
            command: "redo".to_string(),
            description: "Redo".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "X".to_string())],
            command: "cut".to_string(),
            description: "Cut".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "C".to_string())],
            command: "copy".to_string(),
            description: "Copy".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "V".to_string())],
            command: "paste".to_string(),
            description: "Paste".to_string(),
        });

        // Edit operations
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "A".to_string())],
            command: "select_all".to_string(),
            description: "Select All".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "D".to_string())],
            command: "delete_line".to_string(),
            description: "Delete Line".to_string(),
        });

        // Find
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "F".to_string())],
            command: "show_find".to_string(),
            description: "Find".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, true, false, "F".to_string())],
            command: "show_find_replace".to_string(),
            description: "Find and Replace".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "H".to_string())],
            command: "show_find_replace".to_string(),
            description: "Find and Replace".to_string(),
        });

        // Command palette
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, true, false, "P".to_string())],
            command: "show_command_palette".to_string(),
            description: "Command Palette".to_string(),
        });

        // Window
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "W".to_string())],
            command: "new_window".to_string(),
            description: "New Window".to_string(),
        });

        // View
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "B".to_string())],
            command: "toggle_sidebar".to_string(),
            description: "Toggle Sidebar".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(false, false, false, "Escape".to_string())],
            command: "close_panel".to_string(),
            description: "Close Panel".to_string(),
        });

        // Navigation
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "G".to_string())],
            command: "goto_line".to_string(),
            description: "Goto Line".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "P".to_string())],
            command: "goto_file".to_string(),
            description: "Goto File (Quick Open)".to_string(),
        });

        // Multi-cursor
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, true, "D".to_string())],
            command: "find_and_select".to_string(),
            description: "Find and Select".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "D".to_string())],
            command: "find_under_expand".to_string(),
            description: "Find Under Expand".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "K".to_string())],
            command: "find_under".to_string(),
            description: "Find Under".to_string(),
        });

        // Indentation
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(false, false, false, "Tab".to_string())],
            command: "indent".to_string(),
            description: "Indent".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(false, false, true, "Tab".to_string())],
            command: "unindent".to_string(),
            description: "Unindent".to_string(),
        });

        // Split views / Groups (modern editor style)
        // Ctrl+\ splits vertically (side by side)
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "Backslash".to_string())],
            command: "split_vertical".to_string(),
            description: "Split Group Vertically".to_string(),
        });
        // Ctrl+Shift+\ splits horizontally (stacked)
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, true, "Backslash".to_string())],
            command: "split_horizontal".to_string(),
            description: "Split Group Horizontally".to_string(),
        });
        // Ctrl+Q closes all splits
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "Q".to_string())],
            command: "close_all_splits".to_string(),
            description: "Close All Splits".to_string(),
        });

        // Close file operations
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, false, false, "W".to_string())],
            command: "close_file".to_string(),
            description: "Close File".to_string(),
        });
        self.add_binding(KeyBinding {
            keys: vec![KeyCombination::new(true, true, false, "W".to_string())],
            command: "close_all".to_string(),
            description: "Close All Files".to_string(),
        });
    }
}
