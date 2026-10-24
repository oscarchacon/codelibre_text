//! Menu system - JSON-based menus for CodeLibre-style editors

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub id: String,
    pub caption: String,
    pub mnemonic: Option<char>,
    pub command: Option<String>,
    pub args: Option<Value>,
    pub children: Vec<MenuItem>,
    pub visible: bool,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(id: String, caption: String) -> Self {
        Self {
            id,
            caption,
            mnemonic: None,
            command: None,
            args: None,
            children: Vec::new(),
            visible: true,
            enabled: true,
        }
    }
}

/// Menu bar - top level menu
#[derive(Debug, Clone)]
pub struct Menu {
    pub id: String,
    pub caption: String,
    pub items: Vec<MenuItem>,
}

/// A boxed menu handler
pub struct MenuHandler(pub Box<dyn Fn()>);

impl Clone for MenuHandler {
    fn clone(&self) -> Self {
        Self(Box::new(|| ()))
    }
}

impl std::fmt::Debug for MenuHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuHandler").finish()
    }
}

/// Menu manager
#[derive(Debug, Default)]
pub struct MenuManager {
    pub menus: Vec<Menu>,
    pub command_registry: HashMap<String, MenuCommand>,
}

#[derive(Debug, Clone)]
pub struct MenuCommand {
    pub name: String,
    pub caption: String,
    pub handler: MenuHandler,
}

impl MenuManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_defaults(&mut self) {
        // File menu
        self.menus.push(Menu {
            id: "file".to_string(),
            caption: "File".to_string(),
            items: vec![
                MenuItem::new("new_file".to_string(), "New File\tCtrl+N".to_string()),
                MenuItem::new("open_file".to_string(), "Open File...".to_string()).with_command("open_file".to_string()),
                MenuItem::new("separator".to_string(), "-".to_string()),
                MenuItem::new("save_file".to_string(), "Save\tCtrl+S".to_string()),
                MenuItem::new("save_as".to_string(), "Save As...".to_string()),
                MenuItem::new("separator2".to_string(), "-".to_string()),
                MenuItem::new("close_file".to_string(), "Close\tCtrl+W".to_string()),
                MenuItem::new("close_all".to_string(), "Close All".to_string()),
                MenuItem::new("separator3".to_string(), "-".to_string()),
                MenuItem::new("exit".to_string(), "Exit".to_string()),
            ],
        });

        // Edit menu
        self.menus.push(Menu {
            id: "edit".to_string(),
            caption: "Edit".to_string(),
            items: vec![
                MenuItem::new("undo".to_string(), "Undo\tCtrl+Z".to_string()),
                MenuItem::new("redo".to_string(), "Redo\tCtrl+Y".to_string()),
                MenuItem::new("separator1".to_string(), "-".to_string()),
                MenuItem::new("cut".to_string(), "Cut\tCtrl+X".to_string()),
                MenuItem::new("copy".to_string(), "Copy\tCtrl+C".to_string()),
                MenuItem::new("paste".to_string(), "Paste\tCtrl+V".to_string()),
                MenuItem::new("separator2".to_string(), "-".to_string()),
                MenuItem::new("find".to_string(), "Find\tCtrl+F".to_string()),
                MenuItem::new("find_replace".to_string(), "Replace\tCtrl+H".to_string()),
                MenuItem::new("separator3".to_string(), "-".to_string()),
                MenuItem::new("select_all".to_string(), "Select All\tCtrl+A".to_string()),
                MenuItem::new("duplicate_line".to_string(), "Duplicate Line".to_string()),
                MenuItem::new("delete_line".to_string(), "Delete Line".to_string()),
                MenuItem::new("indent".to_string(), "Indent".to_string()),
                MenuItem::new("unindent".to_string(), "Unindent".to_string()),
            ],
        });

        // Selection menu
        self.menus.push(Menu {
            id: "selection".to_string(),
            caption: "Selection".to_string(),
            items: vec![
                MenuItem::new("select_line".to_string(), "Select Line".to_string()),
                MenuItem::new("select_word".to_string(), "Select Word".to_string()),
                MenuItem::new("select_paragraph".to_string(), "Select Paragraph".to_string()),
                MenuItem::new("separator".to_string(), "-".to_string()),
                MenuItem::new("add_cursor_above".to_string(), "Add Cursor Above".to_string()),
                MenuItem::new("add_cursor_below".to_string(), "Add Cursor Below".to_string()),
                MenuItem::new("add_cursor_all_occurrences".to_string(), "Select All Occurrences".to_string()),
                MenuItem::new("expand_selection".to_string(), "Expand Selection".to_string()),
            ],
        });

        // View menu
        self.menus.push(Menu {
            id: "view".to_string(),
            caption: "View".to_string(),
            items: vec![
                MenuItem::new("toggle_sidebar".to_string(), "Toggle Sidebar\tCtrl+B".to_string()),
                MenuItem::new("show_minimap".to_string(), "Show Minimap".to_string()),
                MenuItem::new("toggle_word_wrap".to_string(), "Toggle Word Wrap".to_string()),
                MenuItem::new("zoom_in".to_string(), "Zoom In".to_string()),
                MenuItem::new("zoom_out".to_string(), "Zoom Out".to_string()),
                MenuItem::new("reset_zoom".to_string(), "Reset Zoom".to_string()),
                MenuItem::new("separator".to_string(), "-".to_string()),
                // Split views / Groups
                MenuItem::new("split_view".to_string(), "Split Views".to_string()),
                MenuItem::new("split_vertical".to_string(), "Split Group Right\tCtrl+\\".to_string()),
                MenuItem::new("split_horizontal".to_string(), "Split Group Below\tCtrl+Shift+\\".to_string()),
                MenuItem::new("close_all_splits".to_string(), "Close All Splits\tCtrl+Q".to_string()),
                MenuItem::new("separator2".to_string(), "-".to_string()),
                MenuItem::new("change_syntax".to_string(), "Change Syntax".to_string()),
                MenuItem::new("change_encoding".to_string(), "Change Encoding".to_string()),
                MenuItem::new("line_endings".to_string(), "Line Endings".to_string()),
            ],
        });

        // Find menu
        self.menus.push(Menu {
            id: "find".to_string(),
            caption: "Find".to_string(),
            items: vec![
                MenuItem::new("find_selection".to_string(), "Find Selection\tCtrl+D".to_string()),
                MenuItem::new("find_under_expand".to_string(), "Find Under Expand\tCtrl+D".to_string()),
                MenuItem::new("find_under".to_string(), "Find Under\tCtrl+K Ctrl+D".to_string()),
            ],
        });

        // Tools menu
        self.menus.push(Menu {
            id: "tools".to_string(),
            caption: "Tools".to_string(),
            items: vec![
                MenuItem::new("command_palette".to_string(), "Command Palette\tCtrl+Shift+P".to_string()),
                MenuItem::new("show_panel".to_string(), "Show Panel".to_string()),
                MenuItem::new("separator".to_string(), "-".to_string()),
                MenuItem::new("settings".to_string(), "Settings".to_string()),
                MenuItem::new("keyboard_shortcuts".to_string(), "Keyboard Shortcuts".to_string()),
                MenuItem::new("plugin_manager".to_string(), "Plugin Manager".to_string()),
            ],
        });

        // Help menu
        self.menus.push(Menu {
            id: "help".to_string(),
            caption: "Help".to_string(),
            items: vec![
                MenuItem::new("about".to_string(), "About".to_string()),
                MenuItem::new("documentation".to_string(), "Documentation".to_string()),
            ],
        });
    }

    pub fn register_command(&mut self, name: String, caption: String, handler: MenuHandler) {
        let name_clone = name.clone();
        self.command_registry.insert(name, MenuCommand {
            name: name_clone,
            caption,
            handler,
        });
    }

    pub fn execute_command(&self, command: &str) {
        if let Some(cmd) = self.command_registry.get(command) {
            (cmd.handler.0)();
        }
    }
}

impl MenuItem {
    pub fn with_command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }
}
