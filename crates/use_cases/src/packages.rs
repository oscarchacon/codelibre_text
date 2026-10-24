//! Use cases: load Sublime resources from packages.

use crate::editor::Editor;

impl Editor {
    /// Loads (or reloads) settings, key bindings, menus, palette entries, syntaxes and color schemes.
    pub fn load_packages(&mut self) {
        let mut errors = Vec::new();
        self.state.settings = self.gateways.resources.load_settings(&mut errors);
        let include_sublime = self.global_settings().bool("import_sublime_packages", true);

        let resources = self.gateways.resources.load_ui_resources(include_sublime, &mut errors);
        self.state.keymap = resources.keymap;
        self.state.main_menu = resources.main_menu;
        self.state.palette = resources.palette;
        self.gateways.syntaxes.reload(include_sublime, &mut errors);
        self.state.load_errors = errors;

        // Documents whose syntax no longer exists fall back to a known one.
        for doc in &mut self.state.documents {
            doc.syntax = self.gateways.syntaxes.resolve(&doc.syntax);
        }
        self.refresh_file_index();
    }

    /// `reload_packages`, with a summary in the status bar.
    pub fn reload_packages(&mut self) {
        self.load_packages();
        self.notify(format!(
            "Packages reloaded: {} syntaxes, {} color schemes, {} key bindings",
            self.gateways.syntaxes.syntax_names().len(),
            self.gateways.syntaxes.color_scheme_names().len(),
            self.state.keymap.bindings.len()
        ));
    }

    /// Reloads only the settings layers (after a settings file changed).
    pub fn reload_settings(&mut self) {
        let mut errors = Vec::new();
        self.state.settings = self.gateways.resources.load_settings(&mut errors);
        self.state.load_errors.extend(errors);
    }

    /// `open_file` with a package path, e.g. `${packages}/User/Foo.sublime-settings`.
    pub fn open_package_file(&mut self, file: &str) {
        let path = self.gateways.resources.paths().expand(file);
        self.open_file(&path);
    }

    /// `open_dir`, e.g. Preferences > Browse Packages.
    pub fn open_package_folder(&mut self, dir: &str) {
        let path = self.gateways.resources.paths().expand(dir);
        self.gateways.shell.open_in_file_manager(&path);
    }

    pub fn packages_location(&self) -> (std::path::PathBuf, Option<std::path::PathBuf>) {
        let paths = self.gateways.resources.paths();
        (paths.packages.clone(), paths.sublime_packages.clone())
    }
}
