//! Command Palette - Quick access to all commands and files

use crate::services::keybinding::KeyCombination;
use fuzzy_matcher::FuzzyMatcher;
use std::cmp::Ordering;

/// A command available in the command palette
#[derive(Debug, Clone)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub keys: Vec<String>,
}

/// Command palette state
#[derive(Debug)]
pub struct CommandPalette {
    pub visible: bool,
    pub input: String,
    pub commands: Vec<Command>,
    pub filtered_commands: Vec<usize>,
    pub selected_index: usize,
    pub mode: PaletteMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteMode {
    Commands,
    Files,
    Symbols,
    Lines,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            commands: Vec::new(),
            filtered_commands: Vec::new(),
            selected_index: 0,
            mode: PaletteMode::Commands,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.input.clear();
            self.filtered_commands.clear();
            self.selected_index = 0;
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.input.clear();
        self.filtered_commands.clear();
        self.selected_index = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.input.clear();
        self.filtered_commands.clear();
    }

    pub fn add_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn load_default_commands(&mut self) {
        // File operations
        self.add_command(Command {
            id: "new_file".to_string(),
            name: "New File".to_string(),
            description: "Create a new file".to_string(),
            category: "File".to_string(),
            keys: vec!["Ctrl+N".to_string()],
        });
        self.add_command(Command {
            id: "open_file".to_string(),
            name: "Open File...".to_string(),
            description: "Open an existing file".to_string(),
            category: "File".to_string(),
            keys: vec!["Ctrl+O".to_string()],
        });
        self.add_command(Command {
            id: "save_file".to_string(),
            name: "Save".to_string(),
            description: "Save the current file".to_string(),
            category: "File".to_string(),
            keys: vec!["Ctrl+S".to_string()],
        });
        self.add_command(Command {
            id: "save_as".to_string(),
            name: "Save As...".to_string(),
            description: "Save the current file with a new name".to_string(),
            category: "File".to_string(),
            keys: vec![],
        });
        self.add_command(Command {
            id: "close_file".to_string(),
            name: "Close File".to_string(),
            description: "Close the current file".to_string(),
            category: "File".to_string(),
            keys: vec!["Ctrl+W".to_string()],
        });

        // Edit operations
        self.add_command(Command {
            id: "undo".to_string(),
            name: "Undo".to_string(),
            description: "Undo the last action".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Z".to_string()],
        });
        self.add_command(Command {
            id: "redo".to_string(),
            name: "Redo".to_string(),
            description: "Redo the last undone action".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Y".to_string()],
        });
        self.add_command(Command {
            id: "cut".to_string(),
            name: "Cut".to_string(),
            description: "Cut selected text".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+X".to_string()],
        });
        self.add_command(Command {
            id: "copy".to_string(),
            name: "Copy".to_string(),
            description: "Copy selected text".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+C".to_string()],
        });
        self.add_command(Command {
            id: "paste".to_string(),
            name: "Paste".to_string(),
            description: "Paste from clipboard".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+V".to_string()],
        });
        self.add_command(Command {
            id: "find".to_string(),
            name: "Find".to_string(),
            description: "Find text in file".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+F".to_string()],
        });
        self.add_command(Command {
            id: "find_replace".to_string(),
            name: "Find and Replace".to_string(),
            description: "Find and replace text in file".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+H".to_string()],
        });
        self.add_command(Command {
            id: "select_all".to_string(),
            name: "Select All".to_string(),
            description: "Select all text".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+A".to_string()],
        });
        self.add_command(Command {
            id: "duplicate_line".to_string(),
            name: "Duplicate Line".to_string(),
            description: "Duplicate the current line".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Shift+D".to_string()],
        });
        self.add_command(Command {
            id: "delete_line".to_string(),
            name: "Delete Line".to_string(),
            description: "Delete the current line".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Shift+K".to_string()],
        });
        self.add_command(Command {
            id: "comment_line".to_string(),
            name: "Toggle Line Comment".to_string(),
            description: "Comment or uncomment the current line".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+/".to_string()],
        });
        self.add_command(Command {
            id: "block_comment".to_string(),
            name: "Toggle Block Comment".to_string(),
            description: "Comment or uncomment selected block".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Shift+/".to_string()],
        });

        // View operations
        self.add_command(Command {
            id: "toggle_sidebar".to_string(),
            name: "Toggle Sidebar".to_string(),
            description: "Show or hide the sidebar".to_string(),
            category: "View".to_string(),
            keys: vec!["Ctrl+B".to_string()],
        });
        self.add_command(Command {
            id: "toggle_fullscreen".to_string(),
            name: "Toggle Full Screen".to_string(),
            description: "Toggle full screen mode".to_string(),
            category: "View".to_string(),
            keys: vec!["F11".to_string()],
        });
        self.add_command(Command {
            id: "zoom_in".to_string(),
            name: "Zoom In".to_string(),
            description: "Increase the font size".to_string(),
            category: "View".to_string(),
            keys: vec!["Ctrl+=, Ctrl+Plus".to_string()],
        });
        self.add_command(Command {
            id: "zoom_out".to_string(),
            name: "Zoom Out".to_string(),
            description: "Decrease the font size".to_string(),
            category: "View".to_string(),
            keys: vec!["Ctrl+-".to_string()],
        });

        // Navigation
        self.add_command(Command {
            id: "goto_line".to_string(),
            name: "Go to Line...".to_string(),
            description: "Jump to a specific line number".to_string(),
            category: "Navigation".to_string(),
            keys: vec!["Ctrl+G".to_string()],
        });
        self.add_command(Command {
            id: "goto_symbol".to_string(),
            name: "Go to Symbol...".to_string(),
            description: "Jump to a symbol in the project".to_string(),
            category: "Navigation".to_string(),
            keys: vec!["Ctrl+T".to_string()],
        });
        self.add_command(Command {
            id: "goto_definition".to_string(),
            name: "Go to Definition".to_string(),
            description: "Jump to the definition of the symbol under cursor".to_string(),
            category: "Navigation".to_string(),
            keys: vec!["F12".to_string()],
        });

        // Window
        self.add_command(Command {
            id: "new_window".to_string(),
            name: "New Window".to_string(),
            description: "Open a new editor window".to_string(),
            category: "Window".to_string(),
            keys: vec!["Ctrl+Shift+N".to_string()],
        });
        self.add_command(Command {
            id: "close_window".to_string(),
            name: "Close Window".to_string(),
            description: "Close the current window".to_string(),
            category: "Window".to_string(),
            keys: vec![],
        });

        // Tools
        self.add_command(Command {
            id: "change_syntax".to_string(),
            name: "Change Syntax".to_string(),
            description: "Change the syntax highlighting mode".to_string(),
            category: "Tools".to_string(),
            keys: vec![],
        });
        self.add_command(Command {
            id: "change_encoding".to_string(),
            name: "Change File Encoding".to_string(),
            description: "Change the file encoding".to_string(),
            category: "Tools".to_string(),
            keys: vec![],
        });
        self.add_command(Command {
            id: "install_theme".to_string(),
            name: "Install Theme...".to_string(),
            description: "Browse and install a color theme".to_string(),
            category: "Tools".to_string(),
            keys: vec![],
        });
        self.add_command(Command {
            id: "install_package".to_string(),
            name: "Install Package...".to_string(),
            description: "Browse and install a package".to_string(),
            category: "Tools".to_string(),
            keys: vec![],
        });
    }

    pub fn filter_commands(&mut self) {
        if self.input.is_empty() {
            self.filtered_commands.clear();
            self.filtered_commands.extend(0..self.commands.len());
            self.selected_index = 0;
            return;
        }

        let input_lower = self.input.to_lowercase();
        self.filtered_commands.clear();

        // Fuzzy match the commands
        let mut scored_commands: Vec<(usize, i32)> = Vec::new();

        for (i, cmd) in self.commands.iter().enumerate() {
            let mut score = 0i32;

            // Check if command name contains the input (case insensitive)
            if cmd.name.to_lowercase().contains(&input_lower) {
                score += 100;
            }

            // Fuzzy match
            if let Some(fuzzy_score) = self.fuzzy_match(&cmd.name, &self.input) {
                score += fuzzy_score;
            }

            // Check description
            if cmd.description.to_lowercase().contains(&input_lower) {
                score += 10;
            }

            // Check category
            if cmd.category.to_lowercase().contains(&input_lower) {
                score += 5;
            }

            if score > 0 {
                scored_commands.push((i, score));
            }
        }

        // Sort by score descending
        scored_commands.sort_by(|a, b| b.1.cmp(&a.1));

        self.filtered_commands = scored_commands.into_iter().map(|(i, _)| i).collect();
        if self.selected_index >= self.filtered_commands.len() {
            self.selected_index = if self.filtered_commands.is_empty() { 0 } else { self.filtered_commands.len() - 1 };
        }
    }

    fn fuzzy_match(&self, text: &str, pattern: &str) -> Option<i32> {
        let mut score = 0i32;
        let text_lower = text.to_lowercase();
        let pattern_lower = pattern.to_lowercase();
        let mut text_iter = text_lower.chars().peekable();
        let mut pattern_chars = pattern_lower.chars();

        while let Some(pc) = pattern_chars.next() {
            if let Some(tidx) = text_iter.position(|c| c == pc) {
                score += 1;
                // Bonus for consecutive matches
                if let Some(prev_c) = pattern_chars.clone().next().map(|next_pc| {
                    let remaining: String = text_iter.clone().collect();
                    remaining.find(next_pc).map(|idx| (idx, text_iter.clone().nth(idx).unwrap()))
                }) {
                    score += 1;
                }
            }
        }

        if score > 0 {
            Some(score * 10)
        } else {
            None
        }
    }

    pub fn execute_selected(&self) -> Option<&Command> {
        self.filtered_commands.get(self.selected_index)
            .and_then(|&i| self.commands.get(i))
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty() && self.filtered_commands.is_empty()
    }
}
