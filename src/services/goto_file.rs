//! Goto File / Quick Open - Fuzzy file search
//! Opens a panel to quickly navigate to files in the project

use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;

/// A file found by the quick open search
#[derive(Debug, Clone)]
pub struct QuickOpenEntry {
    pub path: PathBuf,
    pub display_name: String,
    pub relative_path: String,
    /// Score for ranking results
    pub score: i32,
}

/// Theme colors reference for the panel
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: egui::Color32,
    pub fg: egui::Color32,
    pub border: egui::Color32,
    pub line_number: egui::Color32,
    pub command_palette_bg: egui::Color32,
    pub command_palette_fg: egui::Color32,
    pub command_palette_input_bg: egui::Color32,
    pub command_palette_selected: egui::Color32,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(25, 25, 35),
            fg: egui::Color32::from_rgb(200, 200, 200),
            border: egui::Color32::from_rgb(60, 60, 80),
            line_number: egui::Color32::from_rgb(100, 100, 120),
            command_palette_bg: egui::Color32::from_rgb(30, 30, 45),
            command_palette_fg: egui::Color32::from_rgb(220, 220, 220),
            command_palette_input_bg: egui::Color32::from_rgb(40, 40, 55),
            command_palette_selected: egui::Color32::from_rgb(50, 50, 80),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(240, 240, 245),
            fg: egui::Color32::from_rgb(40, 40, 40),
            border: egui::Color32::from_rgb(180, 180, 200),
            line_number: egui::Color32::from_rgb(120, 120, 140),
            command_palette_bg: egui::Color32::from_rgb(250, 250, 255),
            command_palette_fg: egui::Color32::from_rgb(40, 40, 40),
            command_palette_input_bg: egui::Color32::from_rgb(240, 240, 245),
            command_palette_selected: egui::Color32::from_rgb(200, 200, 230),
        }
    }
}

/// State for the Goto File panel
#[derive(Debug)]
pub struct GotoFilePanel {
    pub visible: bool,
    pub input: String,
    pub entries: Vec<QuickOpenEntry>,
    pub selected_index: usize,
    /// All available files in the project
    pub all_files: Vec<PathBuf>,
    /// Project root directories
    pub project_roots: Vec<PathBuf>,
}

impl GotoFilePanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            entries: Vec::new(),
            selected_index: 0,
            all_files: Vec::new(),
            project_roots: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.input.clear();
            self.selected_index = 0;
            self.entries.clear();
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.input.clear();
        self.selected_index = 0;
        self.entries.clear();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.input.clear();
        self.entries.clear();
        self.selected_index = 0;
    }

    /// Populate with all files from project roots
    pub fn discover_files(&mut self, project_roots: Vec<PathBuf>) {
        self.project_roots = project_roots.clone();
        self.all_files.clear();

        for root in &project_roots {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let name = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        // Skip hidden files, binary files, and common non-source files
                        if name.starts_with('.') {
                            continue;
                        }
                        if is_binary_file(&path) {
                            continue;
                        }
                        // Skip node_modules, .git, etc.
                        if path.components().any(|c| {
                            c.as_os_str().to_string_lossy().starts_with("node_modules")
                                || c.as_os_str().to_string_lossy().starts_with(".git")
                                || c.as_os_str().to_string_lossy().starts_with("target")
                                || c.as_os_str().to_string_lossy().starts_with("build")
                        }) {
                            continue;
                        }

                        self.all_files.push(path);
                    } else if path.is_dir() {
                        // Recurse into directories
                        Self::discover_directory(&path, &mut self.all_files, root);
                    }
                }
            }
        }
    }

    fn discover_directory(dir: &PathBuf, collected: &mut Vec<PathBuf>, root: &PathBuf) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    if name.starts_with('.') {
                        continue;
                    }
                    if is_binary_file(&path) {
                        continue;
                    }
                    collected.push(path);
                } else if path.is_dir() {
                    let dir_name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    if dir_name == "node_modules" || dir_name == ".git" || dir_name == "target" || dir_name == "build" {
                        continue;
                    }
                    Self::discover_directory(&path, collected, root);
                }
            }
        }
    }

    /// Filter entries based on fuzzy search input
    pub fn filter_entries(&mut self) {
        if self.input.is_empty() {
            // Show all files when input is empty
            self.entries = self.all_files.iter().map(|path| {
                let display_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let relative_path = self.project_roots.first()
                    .and_then(|root| path.strip_prefix(root).ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());

                QuickOpenEntry {
                    path: path.clone(),
                    display_name,
                    relative_path,
                    score: 0,
                }
            }).collect();
            self.selected_index = 0;
            return;
        }

        let mut scored: Vec<(PathBuf, i32)> = Vec::new();
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();

        for path in &self.all_files {
            let display_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Score based on fuzzy match against filename
            if let Some(score) = matcher.fuzzy_match(&display_name, &self.input) {
                if score > 0 {
                    scored.push((path.clone(), score as i32));
                }
            }

            // Also try matching against relative path
            for root in &self.project_roots {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_str = rel.to_string_lossy();
                    if let Some(score) = matcher.fuzzy_match(&rel_str, &self.input) {
                        if score > 0 {
                            // Higher score for filename match
                            let final_score = if display_name.contains(&self.input.to_lowercase()) {
                                score + 50
                            } else {
                                score
                            };
                            scored.push((path.clone(), final_score as i32));
                            break;
                        }
                    }
                }
            }
        }

        // Sort by score descending
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        self.entries = scored.into_iter().map(|(path, score)| {
            let display_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let relative_path = self.project_roots.first()
                .and_then(|root| path.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());

            QuickOpenEntry {
                path,
                display_name,
                relative_path,
                score,
            }
        }).collect();

        if self.selected_index >= self.entries.len() {
            self.selected_index = if self.entries.is_empty() { 0 } else { self.entries.len() - 1 };
        }
    }

    pub fn get_selected(&self) -> Option<&QuickOpenEntry> {
        self.entries.get(self.selected_index)
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.entries.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.entries.is_empty() {
            self.selected_index = self
                .selected_index
                .checked_sub(1)
                .unwrap_or(self.entries.len() - 1);
        }
    }
}

/// Check if a file is binary (has null bytes in the first 8KB)
fn is_binary_file(path: &PathBuf) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut buffer = [0u8; 8192];
        use std::io::Read;
        if let Ok(bytes_read) = file.read(&mut buffer) {
            for &byte in &buffer[..bytes_read] {
                if byte == 0 {
                    return true;
                }
            }
        }
    }
    false
}

/// Get a file icon based on extension
fn get_file_icon(path: &PathBuf) -> &'static str {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext {
            "rs" => "🦀",
            "py" => "🐍",
            "js" | "mjs" => "📜",
            "ts" => "🔷",
            "html" => "🌐",
            "css" => "🎨",
            "json" => "📋",
            "md" => "📝",
            "txt" => "📄",
            "go" => "🐹",
            "java" => "☕",
            "cpp" | "cc" | "cxx" => "⚙️",
            "h" | "hpp" => "⚙️",
            "rb" => "💎",
            _ => "📄",
        }
    } else {
        "📄"
    }
}

/// Render the Goto File panel in egui
pub fn render_goto_file_panel(
    ctx: &egui::Context,
    panel: &mut GotoFilePanel,
    colors: &ThemeColors,
) {
    use egui::{Area, Id, Order, Frame, Margin};

    if !panel.visible {
        return;
    }

    Area::new(Id::new("goto_file"))
        .order(Order::Foreground)
        .default_pos(egui::pos2(350.0, 80.0))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(colors.command_palette_bg)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .inner_margin(Margin::symmetric(4_i8, 4_i8))
                .outer_margin(Margin::same(0_i8))
                .corner_radius(4_u8)
                .show(ui, |ui| {
                    ui.set_min_width(500.0);
                    ui.set_max_width(600.0);

                    // Input field
                    Frame::NONE
                        .fill(colors.command_palette_input_bg)
                        .inner_margin(Margin::symmetric(8_i8, 6_i8))
                        .outer_margin(Margin::same(0_i8))
                        .show(ui, |ui| {
                            let response = ui.add_sized(
                                egui::vec2(ui.available_width(), 28.0),
                                egui::TextEdit::singleline(&mut panel.input)
                                    .hint_text("Type a filename to search...")
                                    .text_color(colors.command_palette_fg),
                            );
                            response.request_focus();

                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                // Handled by caller
                            }
                        });

                    ui.separator();

                    // Results list
                    for (i, entry) in panel.entries.iter().enumerate() {
                        let is_selected = i == panel.selected_index;

                        Frame::NONE
                            .fill(if is_selected { colors.command_palette_selected } else { colors.command_palette_bg })
                            .inner_margin(Margin::symmetric(8_i8, 4_i8))
                            .outer_margin(Margin::same(0_i8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // File icon
                                    let icon = get_file_icon(&entry.path);
                                    ui.label(egui::RichText::new(icon)
                                        .color(colors.line_number));

                                    // File name
                                    ui.label(egui::RichText::new(&entry.display_name)
                                        .color(colors.command_palette_fg));

                                    // Relative path
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(egui::RichText::new(&entry.relative_path)
                                            .color(colors.line_number)
                                            .monospace()
                                            .size(11.0));
                                    });
                                });
                            });

                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && i > 0 {
                            panel.selected_index = i - 1;
                            ui.scroll_to_cursor(None);
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && i < panel.entries.len() - 1 {
                            panel.selected_index = i + 1;
                            ui.scroll_to_cursor(None);
                        }
                    }
                });
        });
}
