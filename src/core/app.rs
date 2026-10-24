//! Main application - CodeLibre Text editor using egui
//! Soporta vistas divididas y grupos
//! Integrated features: LSP auto-complete, Ctrl+D multi-select, Goto File, macro recording, package manager, extension marketplace

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::domain::buffer::Buffer;
use crate::domain::config::Config;
use crate::ui::editor::EditorView;
use crate::services::find_replace::FindReplacePanel;
use crate::services::command_palette::CommandPalette;
use crate::ui::sidebar::Sidebar;
use crate::domain::split_view::{SplitView, SplitNode, GroupId};
use crate::domain::tab_bar::{TabBar, Tab};
use crate::ui::status_bar::StatusBar;
use crate::domain::settings::SettingsManager;
use crate::services::keybinding::KeybindingManager;
use crate::services::menu::{MenuManager, MenuItem};
use crate::services::plugin::PluginManager;
use crate::services::git::GitProvider;
use crate::domain::theme::ThemeColors;
use crate::domain::project::Project;
use crate::services::tree::build_tree;
use crate::services::undo::UndoManager;
// New modules
use crate::services::lsp::{LspWorkspace, LspClient, CompletionEntry, find_completions, initialize_default_simulated_completions, LspCapabilities};
use crate::services::multiselect::{MultiSelectState, ctrl_d_expand_selection, ctrl_shift_d_duplicate_line};
use crate::services::goto_file::GotoFilePanel;
use crate::services::macro_rec::{MacroRecorder, Modifiers};
use crate::services::package_manager::PackageManager;
use crate::services::extension_marketplace::{ExtensionMarketplace, MarketplaceView};

/// State for a single split group
#[derive(Debug)]
struct GroupState {
    pub id: GroupId,
    pub tabs: TabBar,
}

impl GroupState {
    fn new(id: GroupId) -> Self {
        Self {
            id,
            tabs: TabBar::new(),
        }
    }
}

/// Main application state
pub struct CodeLibreTextApp {
    // Core state
    config: Config,
    settings: SettingsManager,
    undo_manager: UndoManager,

    // Buffers & Editors (shared across groups)
    buffers: HashMap<uuid::Uuid, Buffer>,
    editors: HashMap<uuid::Uuid, EditorView>,

    // Split views / Groups
    split_view: SplitView,
    groups: HashMap<GroupId, GroupState>,

    // UI panels
    sidebar: Sidebar,
    status_bar: StatusBar,
    find_panel: FindReplacePanel,
    command_palette: CommandPalette,

    // Systems
    keybindings: KeybindingManager,
    menu_manager: MenuManager,
    plugin_manager: PluginManager,
    git_provider: GitProvider,
    project: Option<Project>,

    // Window state
    open_dialog: Option<OpenDialog>,
    context_menu: ContextMenu,
    file_history: Vec<PathBuf>,
    open_files_history: Vec<PathBuf>,

    // === NEW FEATURES ===

    // 1. LSP (auto-complete)
    lsp_workspace: LspWorkspace,
    lsp_popup_visible: bool,
    lsp_selected_completion_idx: usize,

    // 2. Multi-select (Ctrl+D)
    multi_select: MultiSelectState,

    // 3. Goto File (Quick Open)
    goto_file_panel: GotoFilePanel,

    // 4. Macro recording
    macro_recorder: MacroRecorder,
    macro_player: crate::services::macro_rec::MacroPlayer,
    is_recording_macro: bool,

    // 5. Package manager
    package_manager: PackageManager,
    show_package_manager: bool,

    // 6. Extension marketplace
    extension_marketplace: ExtensionMarketplace,
}

#[derive(Debug)]
enum OpenDialog {
    OpenFile,
    SaveFile,
}

#[derive(Debug)]
struct ContextMenu {
    visible: bool,
    position: egui::Pos2,
    kind: ContextMenuKind,
}

#[derive(Debug)]
enum ContextMenuKind {
    Tab(GroupId, usize),
    Sidebar,
    Editor,
}

impl CodeLibreTextApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, files_to_open: Vec<String>) -> Self {
        let config = Config::default();
        let mut settings = SettingsManager::new();
        settings.load();

        let mut command_palette = CommandPalette::new();
        command_palette.load_default_commands();

        let mut keybindings = KeybindingManager::new();
        keybindings.load_defaults();

        let mut menu_manager = MenuManager::new();
        menu_manager.load_defaults();

        let plugin_manager = PluginManager::new();

        // Create initial split view with one group
        let split_view = SplitView::new();
        let initial_group_id = match split_view.root() {
            SplitNode::Group(id) => *id,
            _ => 0,
        };

        let mut groups = HashMap::new();
        groups.insert(initial_group_id, GroupState::new(initial_group_id));

        // Initialize LSP workspace with simulated completions
        let mut lsp_workspace = LspWorkspace::new();
        let simulated = initialize_default_simulated_completions();
        for (ext, completions) in simulated {
            lsp_workspace.add_simulated_completions(&ext, completions);
        }

        // Initialize package manager
        let packages_dir = directories::ProjectDirs::from("", "", "codelibre_text")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut package_manager = PackageManager::new();
        package_manager.initialize_default_packages(&packages_dir.join("Packages"));
        package_manager.refresh_sources();

        // Initialize extension marketplace
        let mut extension_marketplace = ExtensionMarketplace::new();
        extension_marketplace.load_default_extensions();

        let mut app = Self {
            config,
            settings,
            undo_manager: UndoManager::new(),
            buffers: HashMap::new(),
            editors: HashMap::new(),
            split_view,
            groups,
            sidebar: Sidebar::new(),
            status_bar: StatusBar::new(),
            find_panel: FindReplacePanel::new(),
            command_palette,
            keybindings,
            menu_manager,
            plugin_manager,
            git_provider: GitProvider::new(),
            project: None,
            open_dialog: None,
            context_menu: ContextMenu {
                visible: false,
                position: egui::Pos2::default(),
                kind: ContextMenuKind::Editor,
            },
            file_history: Vec::new(),
            open_files_history: Vec::new(),

            // New features
            lsp_workspace,
            lsp_popup_visible: false,
            lsp_selected_completion_idx: 0,
            multi_select: MultiSelectState::new(),
            goto_file_panel: GotoFilePanel::new(),
            macro_recorder: MacroRecorder::new(),
            macro_player: crate::services::macro_rec::MacroPlayer::new(),
            is_recording_macro: false,
            package_manager,
            show_package_manager: false,
            extension_marketplace,
        };

        // Create a new untitled buffer by default
        app.new_file();

        // Open files from command line
        for file in files_to_open {
            app.open_file(PathBuf::from(file));
        }

        app
    }

    // ========================================================================
    // Group operations
    // ========================================================================

    /// Split the current active group vertically (side by side)
    fn split_group_vertical(&mut self) {
        if self.split_view.is_single_group() {
            return; // Already single group, no need to split
        }

        // Find the currently active group and split it
        if let Some(active_group_id) = self.get_active_group_id() {
            let new_id = self.split_view.split_active_vertical(active_group_id);
            self.groups.insert(new_id, GroupState::new(new_id));
            // Focus the new group
            self.switch_to_group(new_id);
        }
    }

    /// Split the current active group horizontally (stacked)
    fn split_group_horizontal(&mut self) {
        if self.split_view.is_single_group() {
            return;
        }

        if let Some(active_group_id) = self.get_active_group_id() {
            let new_id = self.split_view.split_active_horizontal(active_group_id);
            self.groups.insert(new_id, GroupState::new(new_id));
            self.switch_to_group(new_id);
        }
    }

    /// Reset all splits - close all but the active group's tabs
    fn close_all_splits(&mut self) {
        if let Some(active_id) = self.get_active_group_id() {
            // Save tabs from active group
            let tabs = std::mem::take(&mut self.groups.get_mut(&active_id).unwrap().tabs.tabs);
            
            // Reset split view
            self.split_view.reset();
            let new_id = match self.split_view.root() {
                SplitNode::Group(id) => *id,
                _ => 0,
            };

            // Clear all groups
            self.groups.clear();
            
            // Create new group with saved tabs
            let mut group = GroupState::new(new_id);
            group.tabs.tabs = tabs;
            self.groups.insert(new_id, group);
            self.split_view = SplitView::new();
        }
    }

    /// Switch focus to a specific group
    fn switch_to_group(&mut self, group_id: GroupId) {
        if let Some(group) = self.groups.get(&group_id) {
            if let Some(active_idx) = group.tabs.active_index() {
                if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                    if let Some(editor) = self.editors.get(&tab.buffer_id) {
                        self.status_bar.update_from_buffer(editor.buffer());
                    }
                }
            }
        }
    }

    /// Get the currently active group ID
    fn get_active_group_id(&self) -> Option<GroupId> {
        // Find which group contains the currently active tab
        let mut all_ids = Vec::new();
        self.split_view.root().group_ids(&mut all_ids);

        for gid in all_ids {
            if let Some(group) = self.groups.get(&gid) {
                if group.tabs.count() > 0 {
                    return Some(gid);
                }
            }
        }
        None
    }

    /// Get the current active group's tabs
    fn active_group_tabs(&self) -> Option<&TabBar> {
        self.get_active_group_id().and_then(|gid| self.groups.get(&gid).map(|g| &g.tabs))
    }

    /// Get mutable reference to current active group's tabs
    fn active_group_tabs_mut(&mut self) -> Option<&mut TabBar> {
        self.get_active_group_id().and_then(|gid| self.groups.get_mut(&gid).map(|g| &mut g.tabs))
    }

    // ========================================================================
    // File operations (per-group)
    // ========================================================================

    fn new_file(&mut self) {
        let buffer = Buffer::new("untitled".to_string(), None);
        let buffer_id = buffer.id();
        self.buffers.insert(buffer_id, buffer.clone());
        self.editors.insert(buffer_id, EditorView::new(buffer));

        if let Some(group_tabs) = self.active_group_tabs_mut() {
            group_tabs.add_tab(buffer_id, "untitled".to_string(), None);
        }
        self.status_bar.file_name = "untitled".to_string();

        // Register LSP client for this buffer
        let client = LspClient::new(buffer_id);
        self.lsp_workspace.register_client(client);
    }

    fn open_file(&mut self, path: PathBuf) {
        if !path.exists() {
            return;
        }

        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string());

        // Check if already open in any group
        for (gid, group) in &self.groups {
            for (tab_idx, tab) in group.tabs.tabs.iter().enumerate() {
                if let Some(buffer) = self.buffers.get(&tab.buffer_id) {
                    if let Some(fp) = buffer.file_path() {
                        if fp == path.to_string_lossy().as_ref() {
                            // Focus existing tab in that group
                            self.focus_group(*gid, tab_idx);
                            return;
                        }
                    }
                }
            }
        }

        // Read file content
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut buffer = Buffer::new(name.clone(), Some(path.to_string_lossy().to_string().into()));
        buffer.content_mut().insert(crate::domain::text_buffer::Position(0), &content);
        buffer.detect_syntax();

        let buffer_id = buffer.id();
        let file_path = buffer.file_path().map(|s| s.to_string());
        self.editors.insert(buffer_id, EditorView::new(buffer.clone()));
        self.buffers.insert(buffer_id, buffer);

        // Register LSP client for this buffer
        let mut client = LspClient::new(buffer_id);
        if let Some(fp) = file_path {
            client.set_document_uri(format!("file://{fp}"));
        }
        self.lsp_workspace.register_client(client);

        if let Some(group_tabs) = self.active_group_tabs_mut() {
            group_tabs.add_tab(buffer_id, name, Some(path.to_string_lossy().to_string()));
        }

        // Update file history
        self.open_files_history.push(path.clone());
        if self.open_files_history.len() > 20 {
            self.open_files_history.drain(..self.open_files_history.len() - 20);
        }
    }

    /// Focus a specific tab in a specific group
    fn focus_group(&mut self, group_id: GroupId, tab_index: usize) {
        if let Some(group) = self.groups.get(&group_id) {
            if tab_index < group.tabs.tabs.len() {
                // Update active tab
                if let Some(group_mut) = self.groups.get_mut(&group_id) {
                    group_mut.tabs.set_active(tab_index);
                }
                // Update status bar
                if let Some(tab) = self.groups.get(&group_id).unwrap().tabs.tab_at_index(tab_index) {
                    if let Some(editor) = self.editors.get(&tab.buffer_id) {
                        self.status_bar.update_from_buffer(editor.buffer());
                    }
                }
            }
        }
    }

    fn save_file(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get_mut(&buffer_id) {
                            if let Some(path) = &buffer.file_path() {
                                if let Err(e) = std::fs::write(path, buffer.content()) {
                                    self.status_bar.set_notification(format!("Error saving: {}", e), 3.0);
                                    return;
                                }
                                buffer.set_modified(false);
                                if let Some(g) = self.groups.get_mut(&gid) {
                                    if let Some(t) = g.tabs.tabs.get_mut(active_idx) {
                                        t.set_modified(false);
                                    }
                                }
                                self.status_bar.set_notification("File saved".to_string(), 2.0);
                            } else {
                                self.open_dialog = Some(OpenDialog::SaveFile);
                            }
                        }
                    }
                }
            }
        }
    }

    fn save_file_as(&mut self, path: PathBuf) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get_mut(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        if let Some(buffer) = self.buffers.get_mut(&tab.buffer_id) {
                            buffer.set_file_path(Some(path.to_string_lossy().to_string().into()));
                            buffer.detect_syntax();
                            self.save_file();
                        }
                    }
                }
            }
        }
    }

    fn save_all(&mut self) {
        let all_group_ids: Vec<GroupId> = self.split_view.all_group_ids().into_iter().collect();
        for gid in all_group_ids {
            if let Some(group) = self.groups.get(&gid) {
                for tab in group.tabs.tabs.iter() {
                    if let Some(buffer) = self.buffers.get(&tab.buffer_id) {
                        if let Some(path) = &buffer.file_path() {
                            let content = buffer.content();
                            let path_clone = path.to_string();
                            if let Err(e) = std::fs::write(&path_clone, content) {
                                self.status_bar.set_notification(format!("Error saving: {}", e), 3.0);
                            }
                        }
                    }
                }
            }
        }
        self.status_bar.set_notification("All files saved".to_string(), 2.0);
    }

    fn close_file(&mut self, index: Option<usize>) {
        let idx = index.or_else(|| {
            self.get_active_group_id().and_then(|gid| {
                self.groups.get(&gid).and_then(|g| g.tabs.active_index())
            })
        });
        
        if let Some(i) = idx {
            if let Some(gid) = self.get_active_group_id() {
                if let Some(group) = self.groups.get_mut(&gid) {
                    group.tabs.remove_tab(i);
                }

                // Update status bar to point to a valid editor
                if let Some(gid) = self.get_active_group_id() {
                    if let Some(group) = self.groups.get(&gid) {
                        if let Some(active) = group.tabs.active_index() {
                            if let Some(tab) = group.tabs.tab_at_index(active) {
                                if let Some(editor) = self.editors.get(&tab.buffer_id) {
                                    self.status_bar.update_from_buffer(editor.buffer());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn close_all_files(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get_mut(&gid) {
                group.tabs.tabs.clear();
                group.tabs.active_tab = None;
            }
            self.new_file();
        }
    }

    // ========================================================================
    // Edit operations
    // ========================================================================

    fn undo(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get_mut(&buffer_id) {
                            if self.undo_manager.undo(buffer.content_mut()) {
                                buffer.set_modified(true);
                            }
                        }
                    }
                }
            }
        }
    }

    fn redo(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get_mut(&buffer_id) {
                            if self.undo_manager.redo(buffer.content_mut()) {
                                buffer.set_modified(true);
                            }
                        }
                    }
                }
            }
        }
    }

    // ========================================================================
    // NEW FEATURE 1: LSP Auto-complete
    // ========================================================================

    fn show_lsp_completions(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get(&buffer_id) {
                            let syntax = buffer.syntax();
                            let completions = find_completions(&self.lsp_workspace, buffer_id, syntax, "");
                            if let Some(client) = self.lsp_workspace.get_client_mut(buffer_id) {
                                client.show_completions(completions);
                                self.lsp_popup_visible = true;
                            }
                        }
                    }
                }
            }
        }
    }

    fn lsp_select_next(&mut self) {
        self.lsp_selected_completion_idx = (self.lsp_selected_completion_idx + 1).min(9);
    }

    fn lsp_select_previous(&mut self) {
        if self.lsp_selected_completion_idx > 0 {
            self.lsp_selected_completion_idx -= 1;
        }
    }

    fn apply_lsp_completion(&mut self, entry: &CompletionEntry) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get_mut(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(editor) = self.editors.get_mut(&buffer_id) {
                            let mut content = editor.buffer_mut().content_mut();
                            let cursors = content.cursors().to_vec();
                            if let Some(cursor) = cursors.first() {
                                content.insert(cursor.position, &entry.insert_text);
                            }
                            // Clear completions after applying
                            self.lsp_popup_visible = false;
                            self.lsp_selected_completion_idx = 0;
                        }
                    }
                }
            }
        }
    }

    fn hide_lsp_completions(&mut self) {
        self.lsp_popup_visible = false;
        self.lsp_selected_completion_idx = 0;
    }

    // ========================================================================
    // NEW FEATURE 2: Multi-select (Ctrl+D)
    // ========================================================================

    fn handle_ctrl_d(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get_mut(&buffer_id) {
                            ctrl_d_expand_selection(&mut buffer.text, &mut self.multi_select);
                        }
                    }
                }
            }
        }
    }

    fn handle_ctrl_shift_d(&mut self) {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        let buffer_id = tab.buffer_id;
                        if let Some(buffer) = self.buffers.get_mut(&buffer_id) {
                            ctrl_shift_d_duplicate_line(&mut buffer.text);
                        }
                    }
                }
            }
        }
    }

    // ========================================================================
    // NEW FEATURE 3: Goto File (Quick Open)
    // ========================================================================

    fn handle_goto_file_input(&mut self) {
        self.goto_file_panel.filter_entries();
    }

    fn open_goto_file_selected(&mut self) {
        if let Some(entry) = self.goto_file_panel.get_selected() {
            self.open_file(entry.path.clone());
            self.goto_file_panel.hide();
        }
    }

    // ========================================================================
    // NEW FEATURE 4: Macro Recording
    // ========================================================================

    fn toggle_macro_recording(&mut self) {
        if self.is_recording_macro {
            self.macro_recorder.stop_recording();
            self.is_recording_macro = false;
            self.status_bar.set_notification("Macro recorded successfully".to_string(), 2.0);
        } else {
            self.macro_recorder.start_recording(None);
            self.is_recording_macro = true;
            self.status_bar.set_notification("Recording macro... (Ctrl+R to stop)".to_string(), 2.0);
        }
    }

    fn play_last_macro(&mut self) {
        if self.macro_recorder.list_macros().is_empty() {
            self.status_bar.set_notification("No macros recorded".to_string(), 2.0);
            return;
        }
        if let Some(last_macro) = self.macro_recorder.list_macros().last() {
            self.macro_player.play(last_macro);
            self.status_bar.set_notification("Playing last macro".to_string(), 2.0);
        }
    }

    fn record_macro_event(&mut self, key: &str, modifiers: Modifiers, character: Option<char>) {
        if self.is_recording_macro {
            if let Some(character) = character {
                self.macro_recorder.record_text(&character.to_string());
            } else {
                self.macro_recorder.record_key(key, modifiers, character);
            }
        }
    }

    // ========================================================================
    // NEW FEATURE 5: Package Manager
    // ========================================================================

    fn toggle_package_manager(&mut self) {
        self.show_package_manager = !self.show_package_manager;
    }

    // ========================================================================
    // NEW FEATURE 6: Extension Marketplace
    // ========================================================================

    fn toggle_extension_marketplace(&mut self) {
        self.extension_marketplace.is_loading = !self.extension_marketplace.is_loading;
    }

    // ========================================================================
    // UI Rendering
    // ========================================================================

    fn theme_colors(&self) -> ThemeColors {
        match self.config.theme {
            crate::domain::config::Theme::Dark => ThemeColors::dark(),
            crate::domain::config::Theme::Light => ThemeColors::light(),
            _ => ThemeColors::dark(),
        }
    }

    fn title_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("title_bar")
            .exact_height(32.0)
            .resizable(false)
            .show(ctx, |ui| {
                let colors = self.theme_colors();
                ui.painter().rect_filled(ui.max_rect(), 0.0, colors.title_bar_bg);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

                    // App icon/name
                    ui.label(egui::RichText::new("CodeLibre Text").color(colors.title_bar_fg));

                    // Menu bar
                    self._render_menu(ui, ctx, colors);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("─").color(colors.title_bar_fg));
                        ui.label(egui::RichText::new("□").color(colors.title_bar_fg));
                        ui.label(egui::RichText::new("✕").color(colors.title_bar_fg));
                    });
                });
            });
    }

    fn _render_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, colors: ThemeColors) {
        // Collect menu data to avoid borrow conflicts
        let menu_data: Vec<(String, Vec<(String, bool, Option<String>)>)> = self.menu_manager.menus.iter()
            .map(|m| {
                let items: Vec<(String, bool, Option<String>)> = m.items.iter()
                    .map(|i| (i.caption.clone(), i.visible, i.command.clone()))
                    .collect();
                (m.caption.clone(), items)
            })
            .collect();

        for (caption, items) in &menu_data {
            ui.menu_button(caption, |ui| {
                for (item_caption, visible, cmd) in items {
                    if *item_caption == "-" {
                        ui.separator();
                    } else {
                        if *visible {
                            if ui.add(egui::Button::new(item_caption)).clicked() {
                                if let Some(ref cmd) = cmd {
                                    self._execute_menu_command(ctx, cmd);
                                }
                                ui.close_menu();
                            }
                        }
                    }
                }
            });
        }
    }

    fn _execute_menu_command(&mut self, _ctx: &egui::Context, command: &str) {
        match command {
            "new_file" => self.new_file(),
            "open_file" => { self.open_dialog = Some(OpenDialog::OpenFile); }
            "save_file" => self.save_file(),
            "save_all" => self.save_all(),
            "close_file" => self.close_file(None),
            "close_all" => self.close_all_files(),
            "undo" => self.undo(),
            "redo" => self.redo(),
            "cut" => {}
            "copy" => {}
            "paste" => {}
            "find" => self.find_panel.toggle(),
            "find_replace" => { self.find_panel.toggle(); self.find_panel.show_replace = true; }
            "select_all" => {
                if let Some(gid) = self.get_active_group_id() {
                    if let Some(group) = self.groups.get(&gid) {
                        if let Some(active_idx) = group.tabs.active_index() {
                            if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                if let Some(editor) = self.editors.get_mut(&tab.buffer_id) {
                                    editor._select_all();
                                }
                            }
                        }
                    }
                }
            }
            "toggle_sidebar" => self.sidebar.toggle_visibility(),
            "split_vertical" => self.split_group_vertical(),
            "split_horizontal" => self.split_group_horizontal(),
            "close_all_splits" => self.close_all_splits(),
            "zoom_in" => {
                if let Some(gid) = self.get_active_group_id() {
                    if let Some(group) = self.groups.get(&gid) {
                        if let Some(active_idx) = group.tabs.active_index() {
                            if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                if let Some(editor) = self.editors.get_mut(&tab.buffer_id) {
                                    editor.zoom_in();
                                }
                            }
                        }
                    }
                }
            }
            "zoom_out" => {
                if let Some(gid) = self.get_active_group_id() {
                    if let Some(group) = self.groups.get(&gid) {
                        if let Some(active_idx) = group.tabs.active_index() {
                            if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                if let Some(editor) = self.editors.get_mut(&tab.buffer_id) {
                                    editor.zoom_out();
                                }
                            }
                        }
                    }
                }
            }
            "command_palette" => self.command_palette.toggle(),
            "install_package" => { self.show_package_manager = !self.show_package_manager; }
            "extension_marketplace" => { self.toggle_extension_marketplace(); }
            "goto_file" => self.goto_file_panel.toggle(),
            "record_macro" => self.macro_recorder.toggle_recording(None),
            _ => {}
        }
    }

    fn sidebar_panel(&mut self, ctx: &egui::Context) {
        if !self.sidebar.is_visible() {
            return;
        }

        egui::SidePanel::left("sidebar")
            .exact_width(self.sidebar.width())
            .resizable(true)
            .show(ctx, |ui| {
                let colors = self.theme_colors();

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Explorer").color(colors.sidebar_fg));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").clicked() {
                            self.new_file();
                        }
                        if ui.small_button("...").clicked() {
                            // More options
                        }
                    });
                });
                ui.separator();

                // Tree view
                let mut files_to_open: Vec<std::path::PathBuf> = Vec::new();
                if let Some(project) = &self.project {
                    for folder in project.folders() {
                        if let Ok(root) = std::fs::read_dir(&folder.path) {
                            ui.label(egui::RichText::new(&folder.path).color(colors.sidebar_fg));
                            ui.separator();

                            for entry in root.flatten() {
                                let path = entry.path();
                                let name = path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();

                                if path.is_file() {
                                    let btn = egui::Button::new(name);
                                    if ui.add(btn).clicked() {
                                        files_to_open.push(path.clone());
                                    }
                                }
                            }
                        }
                    }
                }

                // Open files outside the borrow
                for path in files_to_open {
                    let mut already_open = false;
                    for (_, group) in &self.groups {
                        for tab in &group.tabs.tabs {
                            if let Some(buffer) = self.buffers.get(&tab.buffer_id) {
                                if let Some(fp) = buffer.file_path() {
                                    if fp == path.to_string_lossy().as_ref() {
                                        already_open = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if already_open { break; }
                    }
                    if !already_open {
                        self.open_file(path);
                    }
                }
            });
    }

    fn is_selected_file(&self, path: &PathBuf) -> bool {
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        if let Some(buffer) = self.buffers.get(&tab.buffer_id) {
                            if let Some(fp) = buffer.file_path() {
                                return path.to_string_lossy().to_string() == *fp;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    // ========================================================================
    // Tab bar - per group
    // ========================================================================

    fn tab_bar_for_group(&mut self, ctx: &egui::Context, group_id: GroupId) {
        // Collect tab info first to avoid borrow conflicts
        let tab_info: Vec<(usize, String, bool, bool, uuid::Uuid)> = self.groups.get(&group_id)
            .map(|group| {
                group.tabs.tabs.iter().enumerate().map(|(i, tab)| {
                    (i, tab.display_name().to_string(), group.tabs.active_index() == Some(i), tab.is_modified(), tab.buffer_id)
                }).collect()
            })
            .unwrap_or_default();

        let tab_bg_color = self.theme_colors().tab_bg;
        let tab_fg_color = self.theme_colors().tab_fg;
        let tab_active_bg_color = self.theme_colors().tab_active_bg;
        let tab_active_fg_color = self.theme_colors().tab_active_fg;
        let tab_modified_fg_color = self.theme_colors().tab_modified_fg;

        egui::TopBottomPanel::top(format!("tab_bar_{}", group_id))
            .exact_height(36.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, tab_bg_color);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    for (i, display_name, is_active, is_modified, buffer_id) in &tab_info {
                        let mut bg = tab_bg_color;
                        let mut fg = tab_fg_color;

                        if *is_active {
                            bg = tab_active_bg_color;
                            fg = tab_active_fg_color;
                        }

                        let button = egui::Button::new(format!("  {}  ", display_name))
                            .fill(bg);

                        let response = ui.add(button);

                        if response.clicked() {
                            if let Some(g) = self.groups.get_mut(&group_id) {
                                g.tabs.set_active(*i);
                            }
                            if let Some(editor) = self.editors.get(buffer_id) {
                                self.status_bar.update_from_buffer(editor.buffer());
                            }
                        }

                        if response.double_clicked() {
                            if let Some(g) = self.groups.get_mut(&group_id) {
                                g.tabs.remove_tab(*i);
                            }
                        }

                        if *is_modified {
                            ui.label(egui::RichText::new("●").color(tab_modified_fg_color));
                        }

                        ui.separator();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").clicked() {
                            let buffer = Buffer::new("untitled".to_string(), None);
                            let buffer_id = buffer.id();
                            self.buffers.insert(buffer_id, buffer.clone());
                            self.editors.insert(buffer_id, EditorView::new(buffer));
                            if let Some(g) = self.groups.get_mut(&group_id) {
                                g.tabs.add_tab(buffer_id, "untitled".to_string(), None);
                            }
                            self.status_bar.file_name = "untitled".to_string();
                        }
                    });
                });
            });
    }

    // ========================================================================
    // Editor area - split view rendering
    // ========================================================================

    fn render_editor_area(&mut self, ctx: &egui::Context, node: &SplitNode) {
        match node {
            SplitNode::Group(group_id) => {
                self._render_single_group_editor(ctx, *group_id);
            }
            SplitNode::Split {
                direction,
                left_or_top,
                right_or_bottom,
                proportion,
            } => {
                let colors = self.theme_colors();

                // Get the area for this split
                let painter = ctx.debug_painter();
                let available = ctx.available_rect();

                // Split the area based on proportion
                let split_ratio = *proportion;
                let (left_rect, right_rect) = match direction {
                    crate::domain::split_view::SplitDirection::Vertical => {
                        let total_width = available.width();
                        let split_width = total_width * split_ratio;
                        (
                            egui::Rect::from_min_max(available.min, egui::Pos2::new(available.left() + split_width, available.max.y)),
                            egui::Rect::from_min_max(egui::Pos2::new(available.left() + split_width, available.min.y), available.max),
                        )
                    }
                    crate::domain::split_view::SplitDirection::Horizontal => {
                        let total_height = available.height();
                        let split_height = total_height * split_ratio;
                        (
                            egui::Rect::from_min_max(available.min, egui::Pos2::new(available.max.x, available.top() + split_height)),
                            egui::Rect::from_min_max(egui::Pos2::new(available.min.x, available.top() + split_height), available.max),
                        )
                    }
                };

                // Draw dividers
                painter.rect_stroke(
                    match direction {
                        crate::domain::split_view::SplitDirection::Vertical => {
                            let divider_x = available.left() + split_ratio * available.width();
                            egui::Rect::from_min_max(
                                egui::Pos2::new(divider_x - 2.0, available.min.y),
                                egui::Pos2::new(divider_x + 2.0, available.max.y),
                            )
                        }
                        crate::domain::split_view::SplitDirection::Horizontal => {
                            let divider_y = available.top() + split_ratio * available.height();
                            egui::Rect::from_min_max(
                                egui::Pos2::new(available.min.x, divider_y - 2.0),
                                egui::Pos2::new(available.max.x, divider_y + 2.0),
                            )
                        }
                    },
                    0.0_f32,
                    egui::Stroke::new(2.0_f32, colors.border),
                    egui::epaint::StrokeKind::Inside,
                );

                // Recursively render children in their allocated areas
                self.render_editor_area(ctx, left_or_top);
                self.render_editor_area(ctx, right_or_bottom);
            }
        }
    }

    fn _render_single_group_editor(&mut self, ctx: &egui::Context, group_id: GroupId) {
        // Collect the active tab info first to avoid borrow conflicts
        let active_tab_info = self.groups.get(&group_id)
            .and_then(|g| g.tabs.active_index())
            .and_then(|idx| {
                if let Some(group) = self.groups.get(&group_id) {
                    group.tabs.tab_at_index(idx).map(|t| t.buffer_id)
                } else {
                    None
                }
            });

        if let Some(group) = self.groups.get(&group_id) {
            // Check if this group has tabs
            if group.tabs.count() == 0 {
                // Empty group - show prompt to open a file
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE
                        .fill(self.theme_colors().bg))
                    .show(ctx, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("No open files in this group").color(self.theme_colors().line_number));
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Ctrl+N to create new file").color(self.theme_colors().fg));
                                ui.label(egui::RichText::new("Ctrl+O to open file").color(self.theme_colors().fg));
                            });
                        });
                    });
                return;
            }

            // Show tab bar for this group
            self.tab_bar_for_group(ctx, group_id);
        }

        // Editor area - use a custom panel instead of CentralPanel
        // since we might have multiple splits
        let bg_color = self.theme_colors().bg;
        let border_color = self.theme_colors().border;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE
                .fill(bg_color)
                .stroke(egui::Stroke::new(1.0_f32, border_color)))
            .show(ctx, |ui| {
                // Find the active tab in this group
                if let Some(buffer_id) = active_tab_info {
                    if let Some(editor) = self.editors.get_mut(&buffer_id) {
                        editor.ui(ctx, bg_color);

                        // Process committed text
                        editor.commit_text();
                    }
                }
            });
    }

    fn _welcome_screen(&self, ui: &mut egui::Ui) {
        let colors = self.theme_colors();
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.heading(egui::RichText::new("CodeLibre Text").color(colors.fg));
                ui.separator();
                ui.label(egui::RichText::new("Un editor de código nativo, ligero y de código libre").color(colors.line_number));
                ui.separator();
                ui.add_space(16.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Ctrl+N").color(colors.fg));
                    ui.label("New File");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+O").color(colors.fg));
                    ui.label("Open File");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+S").color(colors.fg));
                    ui.label("Save File");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+P").color(colors.fg));
                    ui.label("Command Palette");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+F").color(colors.fg));
                    ui.label("Find");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+H").color(colors.fg));
                    ui.label("Find & Replace");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+B").color(colors.fg));
                    ui.label("Toggle Sidebar");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+D").color(colors.fg));
                    ui.label("Multi-select");
                    ui.separator();
                    ui.label(egui::RichText::new("Ctrl+R").color(colors.fg));
                    ui.label("Record Macro");
                });
            });
        });
    }

    fn status_bar_panel(&self, ctx: &egui::Context) {
        if !self.config.show_status_bar {
            return;
        }

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(24.0)
            .resizable(false)
            .show(ctx, |ui| {
                let colors = self.theme_colors();
                ui.painter().rect_filled(ui.max_rect(), 0.0, colors.status_bar_bg);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

                    // Recording indicator
                    if self.is_recording_macro {
                        ui.label(egui::RichText::new("● REC")
                            .color(egui::Color32::from_rgb(255, 80, 80)));
                        ui.label(egui::RichText::new(" | ")
                            .color(colors.status_bar_fg).monospace());
                    }

                    ui.label(egui::RichText::new(&self.status_bar.file_name)
                        .color(colors.status_bar_fg));

                    if let Some(branch) = &self.status_bar.git_branch {
                        ui.label(egui::RichText::new(branch)
                            .color(colors.status_bar_fg));
                    }

                    ui.label(egui::RichText::new(" | ")
                        .color(colors.status_bar_fg).monospace());

                    ui.label(egui::RichText::new(&format!("Ln {}, Col {}", self.status_bar.cursor_line, self.status_bar.cursor_column))
                        .color(colors.status_bar_fg));

                    ui.label(egui::RichText::new(" | ")
                        .color(colors.status_bar_fg).monospace());

                    ui.label(egui::RichText::new(&self.status_bar.syntax)
                        .color(colors.status_bar_fg));

                    // LSP status indicator
                    if self.lsp_popup_visible {
                        ui.label(egui::RichText::new(" LSP")
                            .color(egui::Color32::from_rgb(100, 200, 100)));
                    }

                    ui.label(egui::RichText::new(&self.status_bar.encoding)
                        .color(colors.status_bar_fg));

                    ui.label(egui::RichText::new(&format!(" {}", self.status_bar.line_ending.as_str()))
                        .color(colors.status_bar_fg));
                });
            });
    }

    // ========================================================================
    // Command palette with new features
    // ========================================================================

    fn command_palette_panel(&mut self, ctx: &egui::Context) {
        if !self.command_palette.visible {
            return;
        }

        // Add new feature commands if not already added
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "goto_file".to_string(),
            name: "Goto File...".to_string(),
            description: "Quickly open files by name (Quick Open)".to_string(),
            category: "Navigation".to_string(),
            keys: vec!["Ctrl+P".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "record_macro".to_string(),
            name: "Record Macro".to_string(),
            description: "Start/stop recording a keystroke macro".to_string(),
            category: "Tools".to_string(),
            keys: vec!["Ctrl+Shift+R".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "play_macro".to_string(),
            name: "Play Last Macro".to_string(),
            description: "Replay the last recorded macro".to_string(),
            category: "Tools".to_string(),
            keys: vec!["Ctrl+Shift+P".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "package_manager".to_string(),
            name: "Package Manager".to_string(),
            description: "Install and manage packages".to_string(),
            category: "Tools".to_string(),
            keys: vec!["Ctrl+Shift+M".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "extension_marketplace".to_string(),
            name: "Extension Marketplace".to_string(),
            description: "Browse and install extensions".to_string(),
            category: "Tools".to_string(),
            keys: vec!["Ctrl+Shift+E".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "toggle_multiselect".to_string(),
            name: "Multi-select (Ctrl+D)".to_string(),
            description: "Add cursor at next occurrence of word".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+D".to_string()],
        });
        self.command_palette.add_command(crate::services::command_palette::Command {
            id: "duplicate_line".to_string(),
            name: "Duplicate Line".to_string(),
            description: "Duplicate the current line at each cursor".to_string(),
            category: "Edit".to_string(),
            keys: vec!["Ctrl+Shift+D".to_string()],
        });

        egui::Area::new(egui::Id::new("command_palette"))
            .order(egui::Order::Foreground)
            .default_pos(egui::pos2(400.0, 100.0))
            .show(ctx, |ui| {
                let colors = self.theme_colors();

                egui::Frame::NONE
                    .fill(colors.command_palette_bg)
                    .stroke(egui::Stroke::new(1.0_f32, colors.border))
                    .inner_margin(egui::Margin::symmetric(4_i8, 4_i8))
                    .outer_margin(egui::Margin::same(0_i8))
                    .corner_radius(4_u8)
                    .show(ui, |ui| {
                        ui.set_min_width(500.0);
                        ui.set_max_width(600.0);

                        egui::Frame::NONE
                            .fill(colors.command_palette_input_bg)
                            .inner_margin(egui::Margin::symmetric(8_i8, 6_i8))
                            .outer_margin(egui::Margin::same(0_i8))
                            .show(ui, |ui| {
                                let response = ui.add_sized(
                                    egui::vec2(ui.available_width(), 28.0),
                                    egui::TextEdit::singleline(&mut self.command_palette.input)
                                        .hint_text("Type a command or search...")
                                        .text_color(colors.command_palette_fg),
                                );
                                response.request_focus();

                                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    let cmd_opt = self.command_palette.execute_selected();
                                    let cmd_id = cmd_opt.map(|c| c.id.clone());
                                    self.command_palette.hide();
                                    if let Some(ref cmd_id_str) = cmd_id {
                                        self._execute_palette_command_by_id(cmd_id_str);
                                    }
                                }

                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    self.command_palette.hide();
                                }
                            });

                        ui.separator();

                        for (i, &cmd_idx) in self.command_palette.filtered_commands.iter().enumerate() {
                            if let Some(cmd) = self.command_palette.commands.get(cmd_idx) {
                                let is_selected = i == self.command_palette.selected_index;
                                let bg = if is_selected {
                                    colors.command_palette_selected
                                } else {
                                    colors.command_palette_bg
                                };

                                egui::Frame::NONE
                                    .fill(bg)
                                    .inner_margin(egui::Margin::symmetric(8_i8, 4_i8))
                                    .outer_margin(egui::Margin::same(0_i8))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&cmd.name)
                                                .color(colors.command_palette_fg));

                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(&cmd.category)
                                                    .color(colors.line_number)
                                                    .monospace());
                                            });
                                        });
                                    });

                                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && i > 0 {
                                    self.command_palette.selected_index = i - 1;
                                    ui.scroll_to_cursor(None);
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && i < self.command_palette.filtered_commands.len() - 1 {
                                    self.command_palette.selected_index = i + 1;
                                    ui.scroll_to_cursor(None);
                                }
                            }
                        }
                    });
            });
    }

    /// Execute a command selected from the command palette
    fn _execute_palette_command(&mut self, cmd: &crate::services::command_palette::Command) {
        self._execute_palette_command_by_id(&cmd.id);
    }

    fn _execute_palette_command_by_id(&mut self, id: &str) {
        match id {
            "goto_file" => {
                self.goto_file_panel.toggle();
            }
            "record_macro" => {
                self.toggle_macro_recording();
            }
            "play_macro" => {
                self.play_last_macro();
            }
            "package_manager" => {
                self.toggle_package_manager();
            }
            "extension_marketplace" => {
                self.toggle_extension_marketplace();
            }
            "toggle_multiselect" => {
                self.handle_ctrl_d();
            }
            "duplicate_line" => {
                self.handle_ctrl_shift_d();
            }
            _ => {}
        }
    }

    fn find_replace_panel(&mut self, ctx: &egui::Context) {
        if !self.find_panel.visible {
            return;
        }

        egui::Area::new(egui::Id::new("find_replace"))
            .order(egui::Order::Foreground)
            .default_pos(egui::pos2(400.0, 80.0))
            .show(ctx, |ui| {
                let colors = self.theme_colors();

                egui::Frame::NONE
                    .fill(colors.find_panel_bg)
                    .stroke(egui::Stroke::new(1.0_f32, colors.border))
                    .inner_margin(egui::Margin::symmetric(8_i8, 6_i8))
                    .outer_margin(egui::Margin::same(4_i8))
                    .corner_radius(4_u8)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Find").color(colors.find_panel_fg));
                            ui.add_sized(
                                egui::vec2(300.0, 24.0),
                                egui::TextEdit::singleline(&mut self.find_panel.find_text)
                                    .hint_text("Find")
                                    .text_color(colors.find_panel_fg),
                            );

                            if ui.small_button("Aa").clicked() {
                                self.find_panel.set_case_sensitive(!self.find_panel.case_sensitive);
                            }

                            if ui.small_button("W").clicked() {
                                self.find_panel.set_whole_word(!self.find_panel.whole_word);
                            }

                            if ui.small_button(".*").clicked() {
                                self.find_panel.set_use_regex(!self.find_panel.use_regex);
                            }

                            if ui.small_button("↕").clicked() {
                                if self.find_panel.search_direction == crate::services::find_replace::SearchDirection::Forward {
                                    self.find_panel.search_direction = crate::services::find_replace::SearchDirection::Backward;
                                } else {
                                    self.find_panel.search_direction = crate::services::find_replace::SearchDirection::Forward;
                                }
                            }

                            if ui.small_button("◀").clicked() {
                                self.find_panel.previous_match();
                            }

                            if ui.small_button("▶").clicked() {
                                self.find_panel.next_match();
                            }

                            if ui.small_button("✕").clicked() {
                                self.find_panel.toggle();
                            }
                        });

                        if ui.button("Replace").clicked() {
                            self.find_panel.toggle_replace();
                        }

                        if self.find_panel.show_replace {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Replace").color(colors.find_panel_fg));
                                ui.add_sized(
                                    egui::vec2(300.0, 24.0),
                                    egui::TextEdit::singleline(&mut self.find_panel.replace_text)
                                        .hint_text("Replace")
                                        .text_color(colors.find_panel_fg),
                                );

                                if ui.button("Replace").clicked() {
                                    // Replace current match
                                }

                                if ui.button("Replace All").clicked() {
                                    // Replace all matches
                                }
                            });
                        }
                    });
            });
    }

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        for event in input.events.iter() {
            if let egui::Event::Key { key, modifiers, .. } = event {
                let key_name = match key {
                    egui::Key::A => "A",
                    egui::Key::B => "B",
                    egui::Key::C => "C",
                    egui::Key::D => "D",
                    egui::Key::E => "E",
                    egui::Key::F => "F",
                    egui::Key::G => "G",
                    egui::Key::H => "H",
                    egui::Key::I => "I",
                    egui::Key::J => "J",
                    egui::Key::K => "K",
                    egui::Key::L => "L",
                    egui::Key::M => "M",
                    egui::Key::N => "N",
                    egui::Key::O => "O",
                    egui::Key::P => "P",
                    egui::Key::Q => "Q",
                    egui::Key::R => "R",
                    egui::Key::S => "S",
                    egui::Key::T => "T",
                    egui::Key::U => "U",
                    egui::Key::V => "V",
                    egui::Key::W => "W",
                    egui::Key::X => "X",
                    egui::Key::Y => "Y",
                    egui::Key::Z => "Z",
                    egui::Key::Num0 => "0",
                    egui::Key::Num1 => "1",
                    egui::Key::Num2 => "2",
                    egui::Key::Num3 => "3",
                    egui::Key::Num4 => "4",
                    egui::Key::Num5 => "5",
                    egui::Key::Num6 => "6",
                    egui::Key::Num7 => "7",
                    egui::Key::Num8 => "8",
                    egui::Key::Num9 => "9",
                    egui::Key::Backspace => "Backspace",
                    egui::Key::Delete => "Delete",
                    egui::Key::ArrowLeft => "ArrowLeft",
                    egui::Key::ArrowRight => "ArrowRight",
                    egui::Key::ArrowUp => "ArrowUp",
                    egui::Key::ArrowDown => "ArrowDown",
                    egui::Key::Enter => "Enter",
                    egui::Key::Escape => "Escape",
                    egui::Key::Tab => "Tab",
                    egui::Key::Space => "Space",
                    egui::Key::Home => "Home",
                    egui::Key::End => "End",
                    egui::Key::PageUp => "PageUp",
                    egui::Key::PageDown => "PageDown",
                    _ => "unknown",
                };
                let key_comb = crate::services::keybinding::KeyCombination::new(
                    modifiers.ctrl,
                    modifiers.alt,
                    modifiers.shift,
                    key_name.to_string(),
                );

                if let Some(command) = self.keybindings.find_command(&key_comb) {
                    match command {
                        "new_file" => self.new_file(),
                        "open_file" => {
                            self.open_dialog = Some(OpenDialog::OpenFile);
                        }
                        "save_file" => self.save_file(),
                        "undo" => self.undo(),
                        "redo" => self.redo(),
                        "show_find" => self.find_panel.toggle(),
                        "show_find_replace" => {
                            self.find_panel.toggle();
                            self.find_panel.show_replace = true;
                        }
                        "show_command_palette" => self.command_palette.toggle(),
                        "toggle_sidebar" => self.sidebar.toggle_visibility(),
                        "close_panel" => {
                            self.find_panel.toggle();
                            self.command_palette.toggle();
                        }
                        "close_file" => self.close_file(None),
                        "close_all" => self.close_all_files(),
                        "delete_line" => {
                            if let Some(gid) = self.get_active_group_id() {
                                if let Some(group) = self.groups.get(&gid) {
                                    if let Some(active_idx) = group.tabs.active_index() {
                                        if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                            let buffer_id = tab.buffer_id;
                                            if let Some(buffer) = self.buffers.get(&buffer_id) {
                                                if let Some(cursor) = buffer.text.cursors().first() {
                                                    let line_num = buffer.text.line_number_at(cursor.position);
                                                    let lines = buffer.text.lines();
                                                    if line_num < lines.len() {
                                                        let line_start = lines[..line_num].iter().map(|l| l.len() + 1).sum::<usize>();
                                                        let line_end = line_start + lines[line_num].len();
                                                        self.buffers.get_mut(&buffer_id).unwrap().text.delete(line_start..line_end);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "split_vertical" => self.split_group_vertical(),
                        "split_horizontal" => self.split_group_horizontal(),
                        "close_all_splits" => self.close_all_splits(),
                        "indent" => {
                            if let Some(gid) = self.get_active_group_id() {
                                if let Some(group) = self.groups.get(&gid) {
                                    if let Some(active_idx) = group.tabs.active_index() {
                                        if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                            let buffer_id = tab.buffer_id;
                                            if let Some(buffer) = self.buffers.get(&buffer_id) {
                                                if let Some(cursor) = buffer.text.cursors().first() {
                                                    let line_num = buffer.text.line_number_at(cursor.position);
                                                    let insert_pos = buffer.text.position_from_line_col(line_num + 1, 1);
                                                    let indent = "    ";
                                                    self.buffers.get_mut(&buffer_id).unwrap().text.insert(insert_pos, indent);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "unindent" => {
                            // Remove leading whitespace
                        }
                        // NEW FEATURE COMMANDS
                        "find_and_select" => {
                            self.handle_ctrl_d();
                        }
                        "find_under_expand" => {
                            self.handle_ctrl_d();
                        }
                        "find_under" => {
                            // Add cursors to all occurrences
                            if let Some(gid) = self.get_active_group_id() {
                                if let Some(group) = self.groups.get(&gid) {
                                    if let Some(active_idx) = group.tabs.active_index() {
                                        if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                                            let buffer_id = tab.buffer_id;
                                            if let Some(buffer) = self.buffers.get(&buffer_id) {
                                                if let Some(cursor) = buffer.text.cursors().first() {
                                                    let pos = cursor.position;
                                                    let content = buffer.text.content();
                                                    let word = crate::services::multiselect::extract_word_at(content, pos.0);
                                                    if !word.is_empty() {
                                                        crate::services::multiselect::add_cursors_to_all_occurrences(
                                                            &mut self.buffers.get_mut(&buffer_id).unwrap().text,
                                                            &word,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "goto_file" => {
                            self.goto_file_panel.toggle();
                        }
                        "record_macro" => {
                            self.toggle_macro_recording();
                        }
                        "play_macro" => {
                            self.play_last_macro();
                        }
                        "install_package" => {
                            self.toggle_package_manager();
                        }
                        "extension_marketplace" => {
                            self.toggle_extension_marketplace();
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }

        // Handle character input
        for event in input.events.iter() {
            if let egui::Event::Text(text) = event {
                // Record macro event
                for ch in text.chars() {
                    self.record_macro_event("", Modifiers::none(), Some(ch));
                }
            }
        }

        // Handle LSP completion navigation
        if self.lsp_popup_visible {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.hide_lsp_completions();
            } else if let Some(gid) = self.get_active_group_id() {
                if let Some(group) = self.groups.get(&gid) {
                    if let Some(active_idx) = group.tabs.active_index() {
                        if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                            let buffer_id = tab.buffer_id;
                            if let Some(client) = self.lsp_workspace.get_client_mut(buffer_id) {
                                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                    client.select_next();
                                }
                                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                    client.select_previous();
                                }
                                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if let Some(entry) = client.selected_completion() {
                                        let entry_clone = entry.clone();
                                        drop(client);
                                        self.apply_lsp_completion(&entry_clone);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Handle Goto File navigation
        if self.goto_file_panel.visible {
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.open_goto_file_selected();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.goto_file_panel.select_previous();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.goto_file_panel.select_next();
            }
        }

        // Handle Package Manager panel
        if self.show_package_manager {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_package_manager = false;
            }
        }

        // Handle Extension Marketplace
        if self.extension_marketplace.is_loading {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.extension_marketplace.is_loading = false;
            }
        }
    }

    fn update_notifications(&mut self, ctx: &egui::Context) {
        if let Some(_notification) = &self.status_bar.notification {
            let seconds = ctx.input(|i| i.time) as f64;
            if seconds > self.status_bar.notification_timeout {
                self.status_bar.clear_notification();
            }
        }
    }
}

impl eframe::App for CodeLibreTextApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Title bar (shared across all groups)
        self.title_bar(ctx);

        // Sidebar (shared)
        self.sidebar_panel(ctx);

        // Tab bars + editor areas for each group
        let mut all_ids = Vec::new();
        self.split_view.root().group_ids(&mut all_ids);
        
        for &group_id in &all_ids {
            self.tab_bar_for_group(ctx, group_id);
        }

        // Editor area
        self._render_single_group_editor(ctx, self.split_view.all_group_ids().into_iter().next().unwrap_or(0));

        // Status bar (shared)
        self.status_bar_panel(ctx);

        // Command palette (overlay)
        self.command_palette_panel(ctx);

        // Find/Replace panel (overlay)
        self.find_replace_panel(ctx);

        // Goto File panel (overlay)
        if self.goto_file_panel.visible {
            // Process input to filter
            self.handle_goto_file_input();
            crate::services::goto_file::render_goto_file_panel(
                ctx,
                &mut self.goto_file_panel,
                &crate::services::goto_file::ThemeColors::dark(),
            );
        }

        // LSP completion popup (overlay)
        if self.lsp_popup_visible {
            if let Some(gid) = self.get_active_group_id() {
                if let Some(group) = self.groups.get(&gid) {
                    if let Some(active_idx) = group.tabs.active_index() {
                        if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                            let buffer_id = tab.buffer_id;
                            if let Some(client) = self.lsp_workspace.get_client(buffer_id) {
                                crate::services::lsp::render_completion_popup(ctx, client);
                            }
                        }
                    }
                }
            }
        }

        // Package Manager panel (overlay)
        if self.show_package_manager {
            egui::Area::new(egui::Id::new("package_manager"))
                .order(egui::Order::Foreground)
                .default_pos(egui::pos2(500.0, 100.0))
                .show(ctx, |ui| {
                    let colors = crate::services::package_manager::ThemeColors::dark();
                    egui::Frame::NONE
                        .fill(colors.bg)
                        .stroke(egui::Stroke::new(1.0_f32, colors.border))
                        .inner_margin(egui::Margin::symmetric(8_i8, 8_i8))
                        .outer_margin(egui::Margin::same(0_i8))
                        .corner_radius(4_u8)
                        .show(ui, |ui| {
                            ui.set_min_size(egui::Vec2::new(500.0, 400.0));
                            crate::services::package_manager::render_package_manager_panel(ui, &mut self.package_manager, &colors);
                        });
                });
        }

        // Extension Marketplace panel (overlay)
        if self.extension_marketplace.is_loading {
            crate::services::extension_marketplace::render_marketplace_panel(
                ctx,
                &mut self.extension_marketplace,
                &crate::services::extension_marketplace::ThemeColors::dark(),
                |mp, idx| mp.install_extension(idx),
                |mp, idx| mp.uninstall_extension(idx),
                |mp, idx| mp.toggle_enabled(idx),
            );
        }

        // Keyboard shortcuts
        self.handle_keyboard_input(ctx);

        // Update notifications
        self.update_notifications(ctx);

        // Update status bar with active buffer info
        if let Some(gid) = self.get_active_group_id() {
            if let Some(group) = self.groups.get(&gid) {
                if let Some(active_idx) = group.tabs.active_index() {
                    if let Some(tab) = group.tabs.tab_at_index(active_idx) {
                        if let Some(editor) = self.editors.get(&tab.buffer_id) {
                            self.status_bar.update_from_buffer(editor.buffer());
                        }
                    }
                }
            }
        }

        // Auto-save if enabled
        if self.settings.user_settings.auto_save {
            // Auto-save would go here
        }
    }
}
