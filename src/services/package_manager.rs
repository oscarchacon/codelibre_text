//! Package Manager - Install, update, and manage packages for CodeLibre-style editors
//! Packages are stored locally and can be loaded by the plugin system

use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// A package that can be installed
#[derive(Debug, Clone)]
pub struct Package {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub installed: bool,
    pub installed_path: Option<PathBuf>,
    pub repository_url: Option<String>,
    pub dependencies: Vec<String>,
    pub enabled: bool,
}

impl Package {
    pub fn new(name: String, version: String, description: String, author: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            version,
            description,
            author,
            installed: false,
            installed_path: None,
            repository_url: None,
            dependencies: Vec::new(),
            enabled: false,
        }
    }
}

/// Package source - where to fetch packages from
#[derive(Debug, Clone)]
pub struct PackageSource {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

/// Package manager state
#[derive(Debug)]
pub struct PackageManager {
    /// Installed packages
    pub packages: HashMap<String, Package>,
    /// Available packages from sources
    pub available_packages: Vec<Package>,
    /// Package sources
    pub sources: Vec<PackageSource>,
    /// Package installation state
    pub installing: Option<(String, InstallingState)>,
    /// Error messages
    pub errors: Vec<String>,
    /// Search query in the panel
    pub search_query: String,
    /// Active tab (0=Installed, 1=Available)
    pub active_tab: usize,
}

#[derive(Debug, Clone)]
enum InstallingState {
    Downloading,
    Extracting,
    Verifying,
    Done,
    Failed(String),
}

impl PackageManager {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            available_packages: Vec::new(),
            sources: vec![
                PackageSource {
                    name: "Package Control".to_string(),
                    url: "https://packagecontrol.io".to_string(),
                    enabled: true,
                },
            ],
            installing: None,
            errors: Vec::new(),
            search_query: String::new(),
            active_tab: 0,
        }
    }

    /// Initialize with default packages
    pub fn initialize_default_packages(&mut self, packages_dir: &PathBuf) {
        // Create packages directory if it doesn't exist
        if !packages_dir.exists() {
            let _ = std::fs::create_dir_all(packages_dir);
        }

        // Load installed packages from directory
        if let Ok(entries) = std::fs::read_dir(packages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // Try to find package.json in the directory
                    let pkg_json = path.join("package.json");
                    if pkg_json.exists() {
                        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                            if let Ok(pkg_info) = serde_json::from_str::<serde_json::Value>(&content) {
                                let pkg = Package {
                                    id: Uuid::new_v4(),
                                    name: pkg_info.get("name").and_then(|v| v.as_str()).unwrap_or(&name).to_string(),
                                    version: pkg_info.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string(),
                                    description: pkg_info.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    author: pkg_info.get("author").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    installed: true,
                                    installed_path: Some(path.clone()),
                                    repository_url: pkg_info.get("repository").and_then(|v| v.get("url"))
                                        .and_then(|v| v.as_str()).map(|s| s.to_string()),
                                    dependencies: pkg_info.get("dependencies")
                                        .and_then(|v| v.as_object())
                                        .map(|deps| deps.keys().map(|k| k.clone()).collect())
                                        .unwrap_or_default(),
                                    enabled: true,
                                };
                                self.packages.insert(pkg.name.clone(), pkg);
                            }
                        }
                    } else {
                        // No package.json, create a minimal entry
                        let pkg = Package::new(
                            name.clone(),
                            "0.0.0".to_string(),
                            "Local package".to_string(),
                            "Local".to_string(),
                        );
                        let mut pkg = pkg;
                        pkg.installed = true;
                        pkg.enabled = true;
                        pkg.installed_path = Some(path);
                        self.packages.insert(name.clone(), pkg);
                    }
                }
            }
        }
    }

    /// Check for package updates
    pub fn check_updates(&mut self) -> Vec<String> {
        let mut updated = Vec::new();

        for (name, pkg) in &self.packages {
            if let Some(available) = self.available_packages.iter().find(|a| a.name == *name) {
                if available.version > pkg.version {
                    updated.push(format!("{}: {} -> {}", name, pkg.version, available.version));
                }
            }
        }

        updated
    }

    /// Install a package (simulated)
    pub fn install_package(&mut self, package_name: &str) {
        // Check if already installed
        if self.packages.contains_key(package_name) {
            self.errors.push(format!("Package '{}' is already installed", package_name));
            return;
        }

        // Look for package in available sources
        if let Some(available) = self.available_packages.iter().find(|p| p.name == package_name) {
            let mut pkg = available.clone();
            pkg.installed = true;
            pkg.enabled = true;

            // Simulate installation
            let packages_dir = directories::ProjectDirs::from("", "", "codelibre_text")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let pkg_dir = packages_dir.join("Packages").join(&pkg.name);

            if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                if entries.count() > 0 {
                    pkg.installed_path = Some(pkg_dir.clone());
                }
            }

            self.packages.insert(pkg.name.clone(), pkg);
        } else {
            // Package not found in available list, create a placeholder
            let pkg = Package::new(
                package_name.to_string(),
                "0.0.1".to_string(),
                "Package installed via Package Manager".to_string(),
                "Unknown".to_string(),
            );
            self.packages.insert(package_name.to_string(), pkg);
        }
    }

    /// Uninstall a package
    pub fn uninstall_package(&mut self, package_name: &str) {
        if let Some(pkg) = self.packages.remove(package_name) {
            // Try to remove installed files
            if let Some(ref path) = pkg.installed_path {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }

    /// Enable/disable a package
    pub fn set_package_enabled(&mut self, package_name: &str, enabled: bool) {
        if let Some(pkg) = self.packages.get_mut(package_name) {
            pkg.enabled = enabled;
        }
    }

    /// Check if a package is enabled
    pub fn is_package_enabled(&self, package_name: &str) -> bool {
        self.packages.get(package_name).map(|p| p.enabled).unwrap_or(false)
    }

    /// List installed packages
    pub fn list_installed(&self) -> Vec<&Package> {
        self.packages.values().filter(|p| p.installed).collect()
    }

    /// List enabled packages
    pub fn list_enabled(&self) -> Vec<&Package> {
        self.packages.values().filter(|p| p.enabled).collect()
    }

    /// Search available packages
    pub fn search_packages(&self, query: &str) -> Vec<&Package> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        self.available_packages.iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Add a package source
    pub fn add_source(&mut self, source: PackageSource) {
        self.sources.push(source);
    }

    /// Remove a package source
    pub fn remove_source(&mut self, source_name: &str) {
        self.sources.retain(|s| s.name != source_name);
    }

    /// Refresh package list from sources (simulated)
    pub fn refresh_sources(&mut self) {
        // Simulate fetching package list
        self.available_packages = get_default_available_packages();
    }

    /// Refresh filtered packages using search_query
    pub fn filter_packages(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            return; // Show all
        }
        // Re-filter available_packages in-place based on query
        self.available_packages.retain(|p| {
            p.name.to_lowercase().contains(&query)
                || p.description.to_lowercase().contains(&query)
        });
    }

    /// Get current installation progress
    pub fn get_install_status(&self) -> Option<&(String, InstallingState)> {
        self.installing.as_ref()
    }
}

/// Default available packages for the marketplace
fn get_default_available_packages() -> Vec<Package> {
    vec![
        Package::new(
            "Emmet".to_string(),
            "1.0.0".to_string(),
            "Advanced HTML/CSS editor tools".to_string(),
            "Matteo Collina".to_string(),
        ),
        Package::new(
            "Git".to_string(),
            "1.5.0".to_string(),
            "Git integration for diff, blame, and history".to_string(),
            "CodeLibre Team".to_string(),
        ),
        Package::new(
            "Color Highlight".to_string(),
            "2.0.0".to_string(),
            "Highlight CSS color values in your code".to_string(),
            "Nicola Campap".to_string(),
        ),
        Package::new(
            "Babel".to_string(),
            "6.0.0".to_string(),
            "Syntax definitions for Babel JavaScript extensions".to_string(),
            "Babel Team".to_string(),
        ),
        Package::new(
            "Prettier".to_string(),
            "1.23.0".to_string(),
            "Code formatter using Prettier".to_string(),
            "Prettier Team".to_string(),
        ),
        Package::new(
            "Terminal".to_string(),
            "1.0.0".to_string(),
            "Integrated terminal inside the editor".to_string(),
            "CodeLibre Team".to_string(),
        ),
        Package::new(
            "Markdown Preview".to_string(),
            "1.0.0".to_string(),
            "Preview markdown files in a browser".to_string(),
            "CodeLibre Team".to_string(),
        ),
        Package::new(
            "Theme - Dark+".to_string(),
            "3.0.0".to_string(),
            "Dark color theme with enhanced syntax highlighting".to_string(),
            "Theme Team".to_string(),
        ),
        Package::new(
            "AutoFileName".to_string(),
            "1.1.0".to_string(),
            "Auto complete file names".to_string(),
            "hikarln".to_string(),
        ),
        Package::new(
            "SQLight".to_string(),
            "2.1.0".to_string(),
            "SQL syntax highlighting and formatting".to_string(),
            "SQLight Team".to_string(),
        ),
    ]
}

/// Render the Package Manager panel in egui
pub fn render_package_manager_panel(
    ui: &mut egui::Ui,
    manager: &mut PackageManager,
    colors: &ThemeColors,
) {
    use egui::{Frame, Margin, ScrollArea};

    ui.label(egui::RichText::new("Package Manager").color(colors.fg));
    ui.separator();

    // Search field
    let search_response = ui.add(egui::TextEdit::singleline(&mut manager.search_query)
        .hint_text("Search packages...")
        .text_color(colors.fg));
    if search_response.lost_focus() {
        manager.search_packages(&manager.search_query);
    }

    ui.separator();

    // Tabs: Installed / Available
    ui.horizontal(|ui| {
        if ui.selectable_label(manager.active_tab == 0, "Installed").clicked() {
            manager.active_tab = 0;
        }
        if ui.selectable_label(manager.active_tab == 1, "Available").clicked() {
            manager.active_tab = 1;
        }
    });
    ui.separator();

    match manager.active_tab {
        0 => {
            // Installed packages
            let installed: Vec<(String, String, bool)> = manager.list_installed().iter().map(|p| {
                (p.name.clone(), p.version.clone(), p.enabled)
            }).collect();
            if installed.is_empty() {
                ui.label(egui::RichText::new("No packages installed")
                    .color(colors.line_number));
            } else {
                ScrollArea::vertical().show(ui, |ui| {
                    for (name, version, enabled) in &installed {
                        ui.horizontal(|ui| {
                            // Toggle button
                            let label = if *enabled { "ON" } else { "OFF" };
                            let color = if *enabled {
                                egui::Color32::from_rgb(80, 200, 80)
                            } else {
                                egui::Color32::from_rgb(200, 100, 100)
                            };
                            if ui.add(egui::Button::new(label).fill(color)).clicked() {
                                manager.set_package_enabled(name, !enabled);
                            }

                            ui.label(egui::RichText::new(name).color(colors.fg));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(version)
                                    .color(colors.line_number)
                                    .monospace()
                                    .size(11.0));

                                if ui.small_button("×").clicked() {
                                    manager.uninstall_package(name);
                                }
                            });
                        });
                    }
                });
            }
        }
        1 => {
            // Available packages
            if manager.available_packages.is_empty() {
                ui.label(egui::RichText::new("No packages available. Refresh sources.")
                    .color(colors.line_number));
            } else {
                ScrollArea::vertical().show(ui, |ui| {
                    let pkgs: Vec<(String, String, bool)> = manager.available_packages.iter().map(|p| {
                        (p.name.clone(), p.version.clone(), manager.packages.contains_key(&p.name))
                    }).collect();
                    for (name, version, is_installed) in &pkgs {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).color(colors.fg));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(version)
                                    .color(colors.line_number)
                                    .monospace()
                                    .size(11.0));

                                if *is_installed {
                                    ui.label(egui::RichText::new("✓")
                                        .color(egui::Color32::from_rgb(80, 200, 80)));
                                } else if ui.add(egui::Button::new("Install"))
                                    .clicked() {
                                    manager.install_package(name);
                                }
                            });
                        });
                    }
                });
            }
        }
        _ => {}
    }

    // Refresh button
    ui.separator();
    if ui.add(egui::Button::new("Refresh Sources")).clicked() {
        manager.refresh_sources();
    }
}

/// Theme colors reference
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: egui::Color32,
    pub fg: egui::Color32,
    pub border: egui::Color32,
    pub line_number: egui::Color32,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(25, 25, 35),
            fg: egui::Color32::from_rgb(200, 200, 200),
            border: egui::Color32::from_rgb(60, 60, 80),
            line_number: egui::Color32::from_rgb(100, 100, 120),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(240, 240, 245),
            fg: egui::Color32::from_rgb(40, 40, 40),
            border: egui::Color32::from_rgb(180, 180, 200),
            line_number: egui::Color32::from_rgb(120, 120, 140),
        }
    }
}
