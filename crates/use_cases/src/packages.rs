//! Use cases: load Sublime resources from packages, install Package Control.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::editor::Editor;
use crate::gateways::{MessageAnswer, MessageButtons, MessageDialog, MessageLevel};

/// Installed packages (archives, package folders) and their dates, sorted.
pub(crate) type FolderStamp = Vec<(PathBuf, Option<SystemTime>)>;

/// Package Control's latest release (4.x). packagecontrol.io's own link still serves 3.4.1, whose
/// regular expressions Python 3.11 and later reject.
pub const PACKAGE_CONTROL_URL: &str =
    "https://github.com/wbond/package_control/releases/latest/download/Package.Control.sublime-package";
const PACKAGE_CONTROL_FILE: &str = "Package Control.sublime-package";
/// A command Package Control defines: there once it loaded.
const PACKAGE_CONTROL_COMMAND: &str = "install_package";

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
        self.state.context_menus = resources.context_menus;
        self.state.palette = resources.palette;
        self.state.completion_sets = None;
        self.gateways.syntaxes.reload(include_sublime, &mut errors);
        self.state.meta_preferences = self.gateways.resources.load_meta_preferences(include_sublime, &mut errors);
        self.load_translations(&mut errors);
        self.record_errors(&errors);
        self.state.load_errors = errors;

        // Documents whose syntax no longer exists fall back to a known one.
        for doc in &mut self.state.documents {
            doc.syntax = self.gateways.syntaxes.resolve(&doc.syntax);
        }
        self.refresh_file_index();
        self.refresh_menu_build_systems();
        self.state.installed_packages = self.installed_packages_stamp();
        self.state.installed_packages_changed = None;
        self.state.ignored_packages = self.global_settings().get("ignored_packages").cloned();
    }

    /// Package Control disables and enables packages (Disable / Enable Package, and around an
    /// upgrade or a removal) only by changing `ignored_packages`, expecting the editor to unload
    /// or load them at once, as Sublime does: reloads the packages when the setting is not the
    /// one they were loaded with (only the plugins of the packages concerned reload). Called by
    /// [`Editor::check_external_changes`].
    pub fn check_ignored_packages(&mut self) {
        if self.global_settings().get("ignored_packages") != self.state.ignored_packages.as_ref() {
            self.reload_packages();
        }
    }

    /// Our own `Installed Packages` folder (where Package Control puts what it installs).
    fn own_installed_packages(&self) -> Option<PathBuf> {
        let paths = self.gateways.resources.paths();
        paths.archive_folders.iter().find(|f| !f.sublime && !f.shipped).map(|f| f.archives.clone())
    }

    /// The `.sublime-package` files of our `Installed Packages` with their dates, and the folders of
    /// our `Packages` with the date of their `package-metadata.json` (Package Control writes it last
    /// when it installs or upgrades a package it unpacks there, one with `.no-sublime-package`).
    fn installed_packages_stamp(&self) -> FolderStamp {
        let file_system = &self.gateways.file_system;
        let archives = self.own_installed_packages().map(|folder| file_system.list_dir(&folder)).unwrap_or_default();
        let archives = archives
            .into_iter()
            .filter(|entry| !entry.is_dir && entry.path.extension().is_some_and(|e| e == "sublime-package"))
            .map(|entry| {
                let modified = file_system.modified(&entry.path);
                (entry.path, modified)
            });
        let folders = file_system.list_dir(&self.gateways.resources.paths().packages).into_iter().filter(|entry| entry.is_dir).map(|entry| {
            let modified = file_system.modified(&entry.path.join("package-metadata.json"));
            (entry.path, modified)
        });
        let mut stamp: FolderStamp = archives.chain(folders).collect();
        stamp.sort();
        stamp
    }

    /// Reloads the packages when Package Control (or the user) added, upgraded or removed a
    /// `.sublime-package` in our `Installed Packages` or a package folder in our `Packages`, as
    /// Sublime does. It waits until the folders held still for one check: Package Control may be
    /// installing several. The plugin host keeps running (only changed packages reload). Called by
    /// [`Editor::check_external_changes`]. (Package Control also puts a package in
    /// `ignored_packages` while it installs it, but for less than a check when the download is fast.)
    pub fn check_installed_packages(&mut self) {
        let stamp = self.installed_packages_stamp();
        if stamp == self.state.installed_packages {
            self.state.installed_packages_changed = None;
        } else if self.state.installed_packages_changed.as_ref() == Some(&stamp) {
            self.reload_packages();
        } else {
            self.state.installed_packages_changed = Some(stamp);
        }
    }

    /// `install_package_control`: after the user agrees, downloads Package Control into our
    /// `Installed Packages`; [`Editor::poll_downloads`] loads it when it arrives.
    pub fn install_package_control(&mut self) {
        let Some(folder) = self.own_installed_packages() else {
            self.package_control_failed(self.tr("Package Control cannot be installed: there is no Installed Packages folder").to_string());
            return;
        };
        let destination = folder.join(PACKAGE_CONTROL_FILE);
        if self.gateways.file_system.exists(&destination) {
            self.notify(self.tr("Package Control is already installed").to_string());
            return;
        }
        let dialog = MessageDialog {
            level: MessageLevel::Warning,
            title: self.tr("Install Package Control").to_string(),
            text: self.tr_args(
                "Package Control will be downloaded from {url} and run as a plugin. Install it?",
                &[("url", &PACKAGE_CONTROL_URL)],
            ),
            buttons: MessageButtons::OkCancel { ok: self.tr("Install").to_string(), cancel: self.tr("Cancel").to_string() },
        };
        if self.gateways.dialogs.show_message(&dialog) != MessageAnswer::Ok {
            return;
        }
        match self.gateways.downloads.start(PACKAGE_CONTROL_URL, &destination) {
            Ok(()) => self.notify(self.tr("Downloading Package Control…").to_string()),
            Err(error) => self.package_control_failed(self.tr_args("Could not download Package Control: {error}", &[("error", &error)])),
        }
    }

    /// Picks up a finished download (cheap to call every frame). Package Control is installed
    /// only if its commands loaded: a download can arrive and still fail to run (no Python, a
    /// broken archive, a version our Python rejects).
    pub fn poll_downloads(&mut self) {
        match self.gateways.downloads.poll() {
            Some(Ok(())) => {
                self.reload_packages();
                if self.is_plugin_command(PACKAGE_CONTROL_COMMAND) {
                    self.notify(self.tr("Package Control installed: open the Command Palette and type Package Control").to_string());
                } else {
                    self.package_control_failed(
                        self.tr("Package Control was downloaded but did not load (Help > Package Load Errors)").to_string(),
                    );
                }
            }
            Some(Err(error)) => {
                self.package_control_failed(self.tr_args("Could not download Package Control: {error}", &[("error", &error)]));
            }
            None => {}
        }
    }

    /// In the status bar, which it soon leaves, and in the console and the error log, where it stays.
    fn package_control_failed(&mut self, message: String) {
        self.log(&format!("{message}\n"));
        self.record_errors(std::slice::from_ref(&message));
        self.notify(message);
    }

    /// `reload_packages`, with a summary in the status bar: the problems, if any (the reload
    /// after Package Control installs a package is the only sign the user gets of them).
    pub fn reload_packages(&mut self) {
        self.load_packages();
        self.load_plugins();
        let failed = self.state.load_errors.len();
        if failed > 0 {
            self.notify(self.tr_n(
                "{count} package resource failed to load (Help > Package Load Errors)",
                "{count} package resources failed to load (Help > Package Load Errors)",
                failed,
                &[],
            ));
            return;
        }
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
        self.record_errors(&errors);
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
