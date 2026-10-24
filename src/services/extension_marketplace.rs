//! Extension Marketplace - Browse, install, and manage extensions
//! Similar to VS Code's marketplace but for CodeLibre-style packages

use std::collections::HashMap;
use uuid::Uuid;

/// An extension available in the marketplace
#[derive(Debug, Clone)]
pub struct Extension {
    pub id: Uuid,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub display_version: String,
    pub description: String,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub price: String,
    pub rating: f64,
    pub rating_count: usize,
    pub download_count: usize,
    pub installed: bool,
    pub enabled: bool,
    pub repository_url: Option<String>,
    pub icon_url: Option<String>,
    pub screenshots: Vec<String>,
    pub dependencies: Vec<String>,
}

impl Extension {
    pub fn new(
        name: String,
        publisher: String,
        version: String,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            publisher,
            version: version.clone(),
            display_version: version,
            description,
            categories: Vec::new(),
            tags: Vec::new(),
            price: "Free".to_string(),
            rating: 0.0,
            rating_count: 0,
            download_count: 0,
            installed: false,
            enabled: false,
            repository_url: None,
            icon_url: None,
            screenshots: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn category_icon(&self) -> &'static str {
        for cat in &self.categories {
            let icon = match cat.as_str() {
                "Themes" => "🎨",
                "Language Packs" => "🌐",
                "Snippets" => "📋",
                "Linters" => "✅",
                "Formatters" => "✨",
                "Debuggers" => "🐛",
                "Keymaps" => "⌨️",
                "UI" => "🖥️",
                "Tools" => "🔧",
                "Data" => "📊",
                _ => "📦",
            };
            return icon;
        }
        "📦"
    }
}

/// Marketplace state
#[derive(Debug)]
pub struct ExtensionMarketplace {
    /// Available extensions
    pub extensions: Vec<Extension>,
    /// Installed extensions
    pub installed: Vec<Uuid>,
    /// User search query
    pub search_query: String,
    /// Filtered results
    pub filtered: Vec<usize>,
    /// Selected extension index
    pub selected_index: Option<usize>,
    /// View mode
    pub view: MarketplaceView,
    /// Filter options
    pub filter_category: Option<String>,
    pub filter_rating: Option<f64>,
    /// Sorting
    pub sort_by: SortOption,
    /// Loading state
    pub is_loading: bool,
    pub loading_message: String,
    /// Error state
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarketplaceView {
    Browse,
    Search,
    Installed,
    Details(usize),
    TopPicks,
    Trending,
}

#[derive(Debug, Clone, PartialEq)]
enum SortOption {
    Relevance,
    Name,
    Rating,
    Downloads,
    Newest,
}

impl ExtensionMarketplace {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
            installed: Vec::new(),
            search_query: String::new(),
            filtered: Vec::new(),
            selected_index: None,
            view: MarketplaceView::Browse,
            filter_category: None,
            filter_rating: None,
            sort_by: SortOption::Relevance,
            is_loading: false,
            loading_message: String::new(),
            error: None,
        }
    }

    /// Initialize with default extensions
    pub fn load_default_extensions(&mut self) {
        let mut default_extensions = get_marketplace_extensions();

        // Set some as installed
        default_extensions[0].installed = true;
        default_extensions[0].enabled = true;
        default_extensions[0].rating = 4.8;
        default_extensions[0].rating_count = 2847;
        default_extensions[0].download_count = 156000;

        default_extensions[1].installed = true;
        default_extensions[1].enabled = true;
        default_extensions[1].rating = 4.5;
        default_extensions[1].rating_count = 1923;
        default_extensions[1].download_count = 98000;

        default_extensions[2].installed = false;
        default_extensions[2].rating = 4.6;
        default_extensions[2].rating_count = 892;
        default_extensions[2].download_count = 45000;

        default_extensions[3].installed = false;
        default_extensions[3].rating = 4.3;
        default_extensions[3].rating_count = 456;
        default_extensions[3].download_count = 23000;

        default_extensions[4].installed = false;
        default_extensions[4].rating = 4.9;
        default_extensions[4].rating_count = 3421;
        default_extensions[4].download_count = 210000;

        default_extensions[5].installed = false;
        default_extensions[5].rating = 4.1;
        default_extensions[5].rating_count = 234;
        default_extensions[5].download_count = 12000;

        default_extensions[6].installed = false;
        default_extensions[6].rating = 4.7;
        default_extensions[6].rating_count = 1567;
        default_extensions[6].download_count = 78000;

        default_extensions[7].installed = false;
        default_extensions[7].rating = 4.4;
        default_extensions[7].rating_count = 678;
        default_extensions[7].download_count = 34000;

        default_extensions[8].installed = false;
        default_extensions[8].rating = 4.0;
        default_extensions[8].rating_count = 123;
        default_extensions[8].download_count = 8900;

        self.extensions = default_extensions;
        self.filter_and_sort();
    }

    /// Filter and sort extensions based on current criteria
    pub fn filter_and_sort(&mut self) {
        let query_lower = self.search_query.to_lowercase();

        let mut scored: Vec<(usize, i32)> = Vec::new();

        for (i, ext) in self.extensions.iter().enumerate() {
            let mut score = 0i32;

            // Apply filters
            if let Some(ref cat) = self.filter_category {
                if !ext.categories.iter().any(|c| c.to_lowercase() == cat.to_lowercase()) {
                    continue;
                }
            }
            if let Some(min_rating) = self.filter_rating {
                if ext.rating < min_rating {
                    continue;
                }
            }

            // Search scoring
            if !query_lower.is_empty() {
                let name_lower = ext.name.to_lowercase();
                let desc_lower = ext.description.to_lowercase();
                let pub_lower = ext.publisher.to_lowercase();

                if name_lower.contains(&query_lower) {
                    score += 100;
                }
                if desc_lower.contains(&query_lower) {
                    score += 50;
                }
                if pub_lower.contains(&query_lower) {
                    score += 30;
                }

                // Fuzzy match on name
                use fuzzy_matcher::FuzzyMatcher;
                if let Some(fuzzy_score) = fuzzy_matcher::skim::SkimMatcherV2::default()
                    .fuzzy_match(&ext.name, &self.search_query)
                {
                    score += fuzzy_score as i32;
                }
            } else {
                // No search - sort by downloads or rating
                score = ext.download_count as i32 / 1000;
            }

            if score > 0 || query_lower.is_empty() {
                scored.push((i, score));
            }
        }

        // Apply sorting
        match self.sort_by {
            SortOption::Relevance => {
                scored.sort_by(|a, b| b.1.cmp(&a.1));
            }
            SortOption::Name => {
                scored.sort_by(|a, b| {
                    self.extensions[a.0].name.to_lowercase()
                        .cmp(&self.extensions[b.0].name.to_lowercase())
                });
            }
            SortOption::Rating => {
                scored.sort_by(|a, b| {
                    self.extensions[b.0].rating.partial_cmp(&self.extensions[a.0].rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOption::Downloads => {
                scored.sort_by(|a, b| {
                    self.extensions[b.0].download_count.cmp(&self.extensions[a.0].download_count)
                });
            }
            SortOption::Newest => {
                // Use rating_count as proxy for popularity/newness
                scored.sort_by(|a, b| {
                    self.extensions[b.0].rating_count.cmp(&self.extensions[a.0].rating_count)
                });
            }
        }

        self.filtered = scored.into_iter().map(|(i, _)| i).collect();

        // Reset selection
        self.selected_index = None;
    }

    /// Search extensions
    pub fn search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.filter_and_sort();
        self.view = MarketplaceView::Search;
    }

    /// Install an extension
    pub fn install_extension(&mut self, index: usize) {
        if let Some(ext) = self.extensions.get_mut(index) {
            ext.installed = true;
            ext.enabled = true;
            self.installed.push(ext.id);
        }
    }

    /// Uninstall an extension
    pub fn uninstall_extension(&mut self, index: usize) {
        if let Some(ext) = self.extensions.get_mut(index) {
            ext.installed = false;
            ext.enabled = false;
            self.installed.retain(|id| *id != ext.id);
        }
    }

    /// Toggle extension enabled state
    pub fn toggle_enabled(&mut self, index: usize) {
        if let Some(ext) = self.extensions.get_mut(index) {
            ext.enabled = !ext.enabled;
        }
    }

    /// Get filtered extension by index
    pub fn get_extension_at(&self, index: usize) -> Option<&Extension> {
        self.filtered.get(index).and_then(|&i| self.extensions.get(i))
    }

    /// Show extension details
    pub fn show_details(&mut self, index: usize) {
        if index < self.filtered.len() {
            self.selected_index = Some(index);
            self.view = MarketplaceView::Details(index);
        }
    }

    /// Navigate back to browse
    pub fn browse(&mut self) {
        self.view = MarketplaceView::Browse;
        self.search_query.clear();
        self.filter_category = None;
        self.filter_rating = None;
        self.filter_and_sort();
    }

    /// Set category filter
    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.filter_category = category;
        self.filter_and_sort();
    }

    /// Set minimum rating filter
    pub fn set_rating_filter(&mut self, rating: Option<f64>) {
        self.filter_rating = rating;
        self.filter_and_sort();
    }

    /// Set sort option
    pub fn set_sort(&mut self, sort: SortOption) {
        self.sort_by = sort;
        self.filter_and_sort();
    }

    /// Check for updates to installed extensions
    pub fn check_updates(&self) -> Vec<String> {
        // In a real implementation, check against marketplace versions
        Vec::new()
    }

    /// Get install statistics
    pub fn stats(&self) -> (usize, usize) {
        let installed = self.extensions.iter().filter(|e| e.installed).count();
        let enabled = self.extensions.iter().filter(|e| e.enabled).count();
        (installed, enabled)
    }
}

/// Default extensions for the marketplace
fn get_marketplace_extensions() -> Vec<Extension> {
    vec![
        Extension {
            id: Uuid::new_v4(),
            name: "Emmet".to_string(),
            publisher: "Matteo Collina".to_string(),
            version: "1.0.0".to_string(),
            display_version: "1.0.0".to_string(),
            description: "Advanced HTML/CSS editor tools with powerful snippets, smart shortcuts, and real-time code editing.".to_string(),
            categories: vec!["Tools".to_string(), "Snippets".to_string()],
            tags: vec!["emmet".to_string(), "html".to_string(), "css".to_string(), "snippets".to_string()],
            price: "Free".to_string(),
            rating: 4.8,
            rating_count: 2847,
            download_count: 156000,
            installed: true,
            enabled: true,
            repository_url: Some("https://github.com/emmetio/emmet".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Babel - JavaScript (Syntax)".to_string(),
            publisher: "Babel Team".to_string(),
            version: "6.0.0".to_string(),
            display_version: "6.0.0".to_string(),
            description: "Syntax definitions and highlighting for Babel JavaScript extensions including JSX, Flow, and more.".to_string(),
            categories: vec!["Language Packs".to_string()],
            tags: vec!["javascript".to_string(), "babel".to_string(), "syntax".to_string(), "jsx".to_string()],
            price: "Free".to_string(),
            rating: 4.5,
            rating_count: 1923,
            download_count: 98000,
            installed: true,
            enabled: true,
            repository_url: Some("https://github.com/babel/babel".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "GitLens - Git supercharged".to_string(),
            publisher: "GitKraken".to_string(),
            version: "14.0.0".to_string(),
            display_version: "14.0.0".to_string(),
            description: "See beyond the Git blame, visualize code authorship, and understand your codebase like never before.".to_string(),
            categories: vec!["Tools".to_string(), "Debuggers".to_string()],
            tags: vec!["git".to_string(), "blame".to_string(), "diff".to_string()],
            price: "Free".to_string(),
            rating: 4.9,
            rating_count: 3421,
            download_count: 210000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/eamodio/gitlens".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "SQLight".to_string(),
            publisher: "SQLight".to_string(),
            version: "2.1.0".to_string(),
            display_version: "2.1.0".to_string(),
            description: "SQL syntax highlighting, snippets, and formatting. Supports MySQL, PostgreSQL, SQLite, and more.".to_string(),
            categories: vec!["Language Packs".to_string()],
            tags: vec!["sql".to_string(), "database".to_string(), "syntax".to_string()],
            price: "Free".to_string(),
            rating: 4.1,
            rating_count: 234,
            download_count: 12000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/mtxr/vscode-sqltools".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Theme - Dark+".to_string(),
            publisher: "Theme Team".to_string(),
            version: "3.0.0".to_string(),
            display_version: "3.0.0".to_string(),
            description: "Dark color theme with enhanced syntax highlighting for all major programming languages.".to_string(),
            categories: vec!["Themes".to_string()],
            tags: vec!["theme".to_string(), "dark".to_string(), "color".to_string()],
            price: "Free".to_string(),
            rating: 4.7,
            rating_count: 1567,
            download_count: 78000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/sublimehq/Packages".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Color Highlight".to_string(),
            publisher: "Nicola Campap".to_string(),
            version: "2.0.0".to_string(),
            display_version: "2.0.0".to_string(),
            description: "Highlight CSS color values in your code. Supports hex, rgb, rgba, hsl, and named colors.".to_string(),
            categories: vec!["UI".to_string(), "Tools".to_string()],
            tags: vec!["color".to_string(), "css".to_string(), "highlight".to_string()],
            price: "Free".to_string(),
            rating: 4.4,
            rating_count: 678,
            download_count: 34000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/naumovs/color-highlight".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Auto Rename Tag".to_string(),
            publisher: "Jun Han".to_string(),
            version: "1.0.0".to_string(),
            display_version: "1.0.0".to_string(),
            description: "Automatically rename paired HTML/XML tags for faster editing.".to_string(),
            categories: vec!["Tools".to_string()],
            tags: vec!["html".to_string(), "xml".to_string(), "tag".to_string()],
            price: "Free".to_string(),
            rating: 4.0,
            rating_count: 123,
            download_count: 8900,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/formulahendry/auto-rename-tag".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Live Server".to_string(),
            publisher: "Ritwick Dey".to_string(),
            version: "5.7.0".to_string(),
            display_version: "5.7.0".to_string(),
            description: "Launch a local development server with live reload feature for static & dynamic pages.".to_string(),
            categories: vec!["Tools".to_string(), "UI".to_string()],
            tags: vec!["server".to_string(), "live".to_string(), "html".to_string()],
            price: "Free".to_string(),
            rating: 4.6,
            rating_count: 567,
            download_count: 29000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/ritwickdey/vscode-live-server".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
        Extension {
            id: Uuid::new_v4(),
            name: "Todo Tree".to_string(),
            publisher: "Adam Johnson".to_string(),
            version: "0.0.190".to_string(),
            display_version: "0.0.190".to_string(),
            description: "Search your workspace for TODO, FIXME, etc. comments and display them in a tree view.".to_string(),
            categories: vec!["Tools".to_string()],
            tags: vec!["todo".to_string(), "comments".to_string(), "search".to_string()],
            price: "Free".to_string(),
            rating: 4.3,
            rating_count: 445,
            download_count: 21000,
            installed: false,
            enabled: false,
            repository_url: Some("https://github.com/AdamJohnson/todo-tree".to_string()),
            icon_url: None,
            screenshots: vec![],
            dependencies: Vec::new(),
        },
    ]
}

/// Render the Extension Marketplace panel in egui
pub fn render_marketplace_panel(
    ctx: &egui::Context,
    marketplace: &mut ExtensionMarketplace,
    colors: &ThemeColors,
    mut on_install: impl FnMut(&mut ExtensionMarketplace, usize),
    mut on_uninstall: impl FnMut(&mut ExtensionMarketplace, usize),
    mut on_toggle: impl FnMut(&mut ExtensionMarketplace, usize),
) {
    use egui::{Area, Id, Order, Frame, Margin, ScrollArea};

    if !marketplace.is_loading {
        return; // Only show when visible
    }

    Area::new(Id::new("extension_marketplace"))
        .order(Order::Foreground)
        .default_pos(egui::pos2(300.0, 60.0))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(colors.command_palette_bg)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .inner_margin(Margin::symmetric(4_i8, 4_i8))
                .outer_margin(Margin::same(0_i8))
                .corner_radius(4_u8)
                .show(ui, |ui| {
                    ui.set_min_width(550.0);
                    ui.set_max_width(650.0);

                    // Header with search
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Extension Marketplace").color(colors.command_palette_fg));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").clicked() {
                                marketplace.is_loading = false;
                            }
                        });
                    });

                    // Search
                    ui.add(egui::TextEdit::singleline(&mut marketplace.search_query)
                        .hint_text("Search extensions...")
                        .text_color(colors.command_palette_fg));

                    // Tabs
                    ui.horizontal(|ui| {
                        if ui.selectable_label(marketplace.view == MarketplaceView::Browse, "Browse").clicked() {
                            marketplace.browse();
                        }
                        if ui.selectable_label(marketplace.view == MarketplaceView::Search, "Search").clicked() {
                            // Search already handled
                        }
                        if ui.selectable_label(marketplace.view == MarketplaceView::Installed, "Installed").clicked() {
                            marketplace.view = MarketplaceView::Installed;
                        }
                        if ui.selectable_label(marketplace.view == MarketplaceView::TopPicks, "Top Picks").clicked() {
                            marketplace.sort_by = SortOption::Rating;
                            marketplace.filter_and_sort();
                            marketplace.view = MarketplaceView::TopPicks;
                        }
                    });
                    ui.separator();

                    // Results
                    ScrollArea::vertical().show(ui, |ui| {
                        if marketplace.view == MarketplaceView::Installed {
                            // Show installed extensions
                            for &idx in &marketplace.filtered {
                                if let Some(ext) = marketplace.extensions.get(idx) {
                                    if !ext.installed {
                                        continue;
                                    }
                                    render_extension_item(ui, ext, idx, colors, &mut [], &mut on_install, &mut on_uninstall, &mut on_toggle);
                                }
                            }
                        } else {
                            // Show search/browse results
                            for (list_idx, &ext_idx) in marketplace.filtered.iter().enumerate() {
                                if let Some(ext) = marketplace.extensions.get(ext_idx) {
                                    render_extension_item(ui, ext, ext_idx, colors, &mut [], &mut on_install, &mut on_uninstall, &mut on_toggle);
                                }
                            }
                        }
                    });
                });
        });
}

/// Render a single extension card
fn render_extension_item(
    ui: &mut egui::Ui,
    ext: &Extension,
    index: usize,
    colors: &ThemeColors,
    _deps: &mut [String],
    _on_install: &mut impl FnMut(&mut ExtensionMarketplace, usize),
    _on_uninstall: &mut impl FnMut(&mut ExtensionMarketplace, usize),
    _on_toggle: &mut impl FnMut(&mut ExtensionMarketplace, usize),
) {
    ui.horizontal(|ui| {
        // Icon
        let icon = ext.category_icon();
        ui.label(egui::RichText::new(icon).size(16.0));

        // Name and description
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&ext.name)
                    .color(colors.command_palette_fg));
                ui.label(egui::RichText::new(&ext.publisher)
                    .color(colors.line_number)
                    .size(11.0));
            });
            ui.label(egui::RichText::new(&ext.description)
                .color(colors.fg)
                .size(11.0));
        });

        // Stats and actions on the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(&ext.version)
                .color(colors.line_number)
                .monospace()
                .size(10.0));

            // Rating stars
            let rating_display = format!("{:.1}", ext.rating);
            ui.label(egui::RichText::new(rating_display)
                .color(egui::Color32::from_rgb(255, 180, 50))
                .size(10.0));

            if ext.installed {
                if ui.small_button("ON").clicked() {
                    // on_toggle(index);
                }
            } else if ui.add_sized(egui::vec2(60.0, 20.0), egui::Button::new("Install"))
                .clicked() {
                // on_install(index);
            }
        });
    });
    ui.separator();
}

/// Theme colors reference
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: egui::Color32,
    pub fg: egui::Color32,
    pub border: egui::Color32,
    pub line_number: egui::Color32,
    pub command_palette_bg: egui::Color32,
    pub command_palette_fg: egui::Color32,
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
        }
    }
}
