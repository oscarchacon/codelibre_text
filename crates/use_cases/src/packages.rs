//! Use cases: load Sublime resources from packages.

use crate::editor::Editor;

impl Editor {
    /// Loads (or reloads) settings, key bindings, menus, palette entries, syntaxes and color schemes.
    pub fn load_packages(&mut self) {
        let mut errors = Vec::new();
        // First, since unpacked packages have settings too.
        let import_sublime = self.gateways.resources.read_user_settings().get("import_sublime_packages").and_then(|v| v.as_bool());
        self.gateways.resources.unpack_packages(import_sublime.unwrap_or(true), &mut errors);
        self.state.settings = self.gateways.resources.load_settings(&mut errors);
        let include_sublime = self.global_settings().bool("import_sublime_packages", true);

        let resources = self.gateways.resources.load_ui_resources(include_sublime, &mut errors);
        self.state.keymap = resources.keymap;
        self.state.main_menu = resources.main_menu;
        self.state.palette = resources.palette;
        self.gateways.syntaxes.reload(include_sublime, &mut errors);
        self.load_translations(&mut errors);
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
        self.load_plugins();
        self.notify(self.tr_args(
            "Packages reloaded: {syntaxes} syntaxes, {schemes} color schemes, {bindings} key bindings, {commands} plugin commands",
            &[
                ("syntaxes", &self.gateways.syntaxes.syntax_names().len()),
                ("schemes", &self.gateways.syntaxes.color_scheme_names().len()),
                ("bindings", &self.state.keymap.bindings.len()),
                ("commands", &self.state.plugins.commands.len()),
            ],
        ));
    }

    /// Reloads only the Python plugins (after a plugin file was saved).
    pub fn reload_plugins(&mut self) {
        self.load_plugins();
        let failed = self.state.plugins.errors.len();
        if failed > 0 {
            self.notify(self.tr_n(
                "{count} plugin error (View > Show Console)",
                "{count} plugin errors (View > Show Console)",
                failed,
                &[],
            ));
        } else {
            self.notify(self.tr_n(
                "Plugins reloaded: {count} command",
                "Plugins reloaded: {count} commands",
                self.state.plugins.commands.len(),
                &[],
            ));
        }
    }

    /// Reloads only the settings layers (after a settings file changed), and the translations
    /// when the `language` setting changed.
    pub fn reload_settings(&mut self) {
        let language = self.language_setting();
        let mut errors = Vec::new();
        self.state.settings = self.gateways.resources.load_settings(&mut errors);
        if self.language_setting() != language {
            self.load_translations(&mut errors);
        }
        self.state.load_errors.extend(errors);
        // Files plugins opened are read again, unless they hold changes not saved yet.
        self.state.plugins.settings_files.retain(|_, file| !file.changes.is_empty());
    }

    /// `open_file` with a package path, e.g. `${packages}/User/Foo.sublime-settings`. With
    /// `encoded_position` the name may end in `:line` or `:line:column` (one-based).
    pub fn open_package_file(&mut self, file: &str, encoded_position: bool) {
        let (file, position) = if encoded_position { split_position(file) } else { (file, None) };
        let path = self.gateways.resources.paths().expand(file);
        if self.open_file(&path) {
            if let Some((line, column)) = position {
                self.go_to_position(line.saturating_sub(1), column.saturating_sub(1));
            }
        }
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

/// `C:\a.txt:12:3` → (`C:\a.txt`, Some((12, 3))); `a.txt:12` → (`a.txt`, Some((12, 1))).
fn split_position(file: &str) -> (&str, Option<(usize, usize)>) {
    let number = |s: &str| s.parse::<usize>().ok().filter(|_| s.bytes().all(|b| b.is_ascii_digit()));
    if let Some((rest, last)) = file.rsplit_once(':') {
        if let Some(last) = number(last) {
            if let Some((name, line)) = rest.rsplit_once(':') {
                if let Some(line) = number(line) {
                    return (name, Some((line, last)));
                }
            }
            return (rest, Some((last, 1)));
        }
    }
    (file, None)
}
