//! Tests for the package use cases (`use_cases::packages`).

use std::path::PathBuf;

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use entities::menu::{MenuItem, PaletteCommand};
use serde_json::json;
use test_support::{status, World, INSTALLED_PACKAGES, PACKAGES};
use use_cases::gateways::PluginCommandKind::Window;
use use_cases::gateways::{expand_variable, MessageAnswer, PluginMessage};
use use_cases::packages::PACKAGE_CONTROL_URL;

fn save_binding() -> Binding {
    Binding { keys: vec![KeyCombo::parse("ctrl+s").unwrap()], command: Command::new("save"), context: vec![] }
}

#[test]
fn startup_loads_key_bindings_menus_and_palette_entries() {
    let world = World::new()
        .with_binding(save_binding())
        .with_menu(MenuItem { caption: "File".into(), id: Some("file".into()), command: None, checkbox: false, children: vec![] })
        .with_palette(PaletteCommand { caption: "File: Save".into(), command: Command::new("save") });
    let editor = world.editor();
    let state = editor.state();
    assert_eq!(state.keymap.bindings.len(), 1);
    assert_eq!(state.main_menu[0].caption, "File");
    assert_eq!(state.palette[0].caption, "File: Save");
}

#[test]
fn sublime_packages_are_included_by_default() {
    let world = World::new();
    world.editor();
    assert_eq!(*world.resources.ui_loads.borrow(), vec![true]);
    assert_eq!(*world.syntaxes.reloads.borrow(), vec![true]);
}

#[test]
fn import_sublime_packages_false_leaves_them_out() {
    let world = World::new().with_preferences(json!({ "import_sublime_packages": false }));
    world.editor();
    assert_eq!(*world.resources.ui_loads.borrow(), vec![false]);
    assert_eq!(*world.syntaxes.reloads.borrow(), vec![false]);
}

#[test]
fn load_errors_are_kept_and_announced() {
    let world = World::new();
    world.resources.errors.borrow_mut().push("bad.sublime-keymap: expected `]`".into());
    let editor = world.editor();
    assert_eq!(editor.state().load_errors, vec!["bad.sublime-keymap: expected `]`"]);
    assert!(status(&editor).contains("1 package resource failed to load"));
}

#[test]
fn reload_packages_reads_everything_again_and_reports_counts() {
    let world = World::new().with_binding(save_binding());
    let mut editor = world.editor();
    world.resources.bindings.borrow_mut().push(save_binding());
    editor.reload_packages();
    assert_eq!(editor.state().keymap.bindings.len(), 2);
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    assert_eq!(status(&editor), "Packages reloaded: 6 syntaxes, 3 color schemes, 2 key bindings, 0 plugin commands");
}

/// The first start unpacks everything Sublime Text ships: the window does not wait for it.
#[test]
fn startup_unpacks_in_the_background_and_loads_the_packages_again_when_it_changed_them() {
    let world = World::new();
    world.resources.background_unpacking.set(true);
    let mut editor = world.editor();
    assert!(world.resources.unpacks.borrow().is_empty(), "nothing is unpacked while starting");
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "the packages load from what was unpacked before");
    assert_eq!(status(&editor), "Unpacking the installed packages…");

    editor.poll_unpacking();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "still unpacking");

    world.resources.unpacked.set(Some(true));
    editor.poll_unpacking();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    assert!(status(&editor).starts_with("Packages reloaded"), "{}", status(&editor));
    assert_eq!(world.resources.unpacks.borrow().len(), 1, "loading unpacks again once the background is done");

    world.resources.unpacked.set(Some(true));
    editor.poll_unpacking();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2, "nothing is being unpacked any more");
}

/// Every start but the first, and the one after Sublime Text or a package changed.
#[test]
fn an_unpacking_that_changed_nothing_does_not_load_the_packages_again() {
    let world = World::new();
    world.resources.background_unpacking.set(true);
    world.resources.unpacked.set(Some(false));
    let mut editor = world.editor();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1);
    assert_ne!(status(&editor), "Unpacking the installed packages…", "it ended while starting");
    editor.reload_packages();
    assert_eq!(world.resources.unpacks.borrow().len(), 1, "loading unpacks again");
}

/// Two unpackings must not write the same folders: a load during the background one leaves the
/// archives alone, and is done again at the end (an archive that arrived since was missed).
#[test]
fn packages_loaded_while_unpacking_are_loaded_again_when_it_ends() {
    let world = World::new();
    world.resources.background_unpacking.set(true);
    let mut editor = world.editor();
    editor.reload_packages();
    assert!(world.resources.unpacks.borrow().is_empty());
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);

    world.resources.unpacked.set(Some(false));
    editor.poll_unpacking();
    assert_eq!(world.resources.ui_loads.borrow().len(), 3);
    assert_eq!(world.resources.unpacks.borrow().len(), 1);
}

#[test]
fn reload_settings_only_reads_the_settings() {
    let world = World::new();
    let mut editor = world.editor();
    world.resources.preferences.borrow_mut().push(json!({ "tab_size": 3 }));
    editor.reload_settings();
    assert_eq!(editor.global_settings().usize("tab_size", 0), 3);
    assert_eq!(world.resources.ui_loads.borrow().len(), 1);
}

#[test]
fn loading_packages_rescans_the_folders() {
    let world = World::new();
    world.editor();
    assert_eq!(world.file_index.rebuilds.borrow().len(), 1);
}

#[test]
fn open_package_file_expands_the_packages_variable() {
    let world = World::new().with_file("/packages/User/notes.txt", "hi");
    let mut editor = world.editor();
    editor.open_package_file("${packages}/User/notes.txt", false);
    assert_eq!(editor.active_document().unwrap().text, "hi");
}

#[test]
fn the_packages_variable_also_expands_without_braces() {
    let world = World::new().with_file("/packages/User/Package Control.user-ca-bundle", "certs");
    let mut editor = world.editor();
    editor.open_package_file("$packages/User/Package Control.user-ca-bundle", false);
    assert_eq!(editor.active_document().unwrap().text, "certs");
    assert_eq!(expand_variable("$packages_x/$packages", "packages", "/p"), "$packages_x//p", "only the whole name");
}

#[test]
fn open_package_folder_opens_the_file_manager() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_package_folder("${packages}", "");
    assert_eq!(*world.opened_folders.borrow(), vec![PathBuf::from("/packages")]);
}

#[test]
fn open_package_folder_selects_the_file_it_is_given() {
    let world = World::new().with_file("/installed/Emmet.sublime-package", "");
    let mut editor = world.editor();
    editor.open_package_folder("/installed", "Emmet.sublime-package");
    assert_eq!(*world.selected_files.borrow(), vec![PathBuf::from("/installed/Emmet.sublime-package")]);
    assert!(world.opened_folders.borrow().is_empty());

    // A file that is not there: the folder alone.
    editor.open_package_folder("/installed", "Gone.sublime-package");
    assert_eq!(*world.opened_folders.borrow(), vec![PathBuf::from("/installed")]);
}

fn package_control_file() -> PathBuf {
    PathBuf::from(INSTALLED_PACKAGES).join("Package Control.sublime-package")
}

#[test]
fn install_package_control_asks_then_downloads_it_into_installed_packages() {
    let world = World::new().with_installed_packages();
    world.dialogs.message_answers.borrow_mut().push_back(MessageAnswer::Ok);
    let mut editor = world.editor();
    editor.install_package_control();
    let asked = world.dialogs.messages.borrow();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].text.contains(PACKAGE_CONTROL_URL));
    assert_eq!(*world.downloads.started.borrow(), vec![(PACKAGE_CONTROL_URL.to_string(), package_control_file())]);
    assert_eq!(status(&editor), "Downloading Package Control…");
}

#[test]
fn install_package_control_does_nothing_when_the_user_cancels() {
    let world = World::new().with_installed_packages();
    let mut editor = world.editor();
    editor.install_package_control();
    assert!(world.downloads.started.borrow().is_empty());
}

#[test]
fn install_package_control_when_it_is_there_only_says_so() {
    let world = World::new().with_installed_packages();
    world.disk.put(package_control_file(), "zip");
    let mut editor = world.editor();
    editor.install_package_control();
    assert!(world.dialogs.messages.borrow().is_empty());
    assert!(world.downloads.started.borrow().is_empty());
    assert_eq!(status(&editor), "Package Control is already installed");
}

#[test]
fn a_finished_download_reloads_the_packages_and_package_control_loads() {
    let world = World::new().with_installed_packages().with_plugin_commands(&[("other", Window)]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Loaded {
        commands: vec![("install_package".into(), Window)],
        events: Vec::new(),
        errors: Vec::new(),
    }]);
    *world.downloads.finished.borrow_mut() = Some(Ok(()));
    editor.poll_downloads();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    assert!(status(&editor).starts_with("Package Control installed"), "{}", status(&editor));
}

#[test]
fn a_download_that_does_not_load_is_not_called_installed() {
    let world = World::new().with_installed_packages();
    let mut editor = world.editor();
    *world.downloads.finished.borrow_mut() = Some(Ok(()));
    editor.poll_downloads();
    let message = "Package Control was downloaded but did not load (Help > Package Load Errors)";
    assert_eq!(status(&editor), message);
    assert!(editor.state().console.contains(message));
}

#[test]
fn a_failed_download_is_reported_and_kept_in_the_console() {
    let world = World::new().with_installed_packages();
    let mut editor = world.editor();
    *world.downloads.finished.borrow_mut() = Some(Err("curl: (6) Could not resolve host: github.com".into()));
    editor.poll_downloads();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1);
    let message = "Could not download Package Control: curl: (6) Could not resolve host: github.com";
    assert_eq!(status(&editor), message);
    assert!(editor.state().console.contains(message));
    assert_eq!(*world.logged_errors.borrow(), vec![message.to_string()]);
}

#[test]
fn install_package_control_without_installed_packages_says_why() {
    let world = World::new();
    let mut editor = world.editor();
    editor.install_package_control();
    assert_eq!(status(&editor), "Package Control cannot be installed: there is no Installed Packages folder");
    assert!(world.downloads.started.borrow().is_empty());
}

#[test]
fn a_reload_with_problems_says_so() {
    let world = World::new();
    let mut editor = world.editor();
    world.resources.errors.borrow_mut().push("Broken.sublime-package: invalid Zip archive".into());
    editor.reload_packages();
    assert_eq!(status(&editor), "1 package resource failed to load (Help > Package Load Errors)");
}

#[test]
fn a_new_installed_package_reloads_the_packages_once_the_folder_holds_still() {
    let world = World::new().with_installed_packages();
    let mut editor = world.editor();
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "nothing changed");

    world.disk.put(format!("{INSTALLED_PACKAGES}/Theme.sublime-package"), "zip");
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "it may still be changing");
    world.disk.put(format!("{INSTALLED_PACKAGES}/Linter.sublime-package"), "zip");
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "it changed again");
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2, "loaded already");
}

/// Package Control unpacks a package with `.no-sublime-package` into `Packages`: installed,
/// upgraded (it writes `package-metadata.json` last) and removed, each reloads the packages.
#[test]
fn a_package_folder_installed_upgraded_or_removed_reloads_the_packages() {
    let world = World::new();
    world.disk.put(format!("{PACKAGES}/User/Preferences.sublime-settings"), "{}");
    let mut editor = world.editor();
    let loads = || world.resources.ui_loads.borrow().len();
    let check_twice = |editor: &mut use_cases::Editor| {
        editor.check_installed_packages();
        editor.check_installed_packages();
    };

    world.disk.put(format!("{PACKAGES}/Terminal/plugin.py"), "");
    world.disk.put(format!("{PACKAGES}/Terminal/package-metadata.json"), "{\"version\": \"1.0\"}");
    check_twice(&mut editor);
    assert_eq!(loads(), 2, "installed");

    world.disk.put(format!("{PACKAGES}/Terminal/plugin.py"), "# 2.0");
    world.disk.put(format!("{PACKAGES}/Terminal/package-metadata.json"), "{\"version\": \"2.0\"}");
    check_twice(&mut editor);
    assert_eq!(loads(), 3, "upgraded");

    world.disk.remove_dir(format!("{PACKAGES}/Terminal"));
    check_twice(&mut editor);
    assert_eq!(loads(), 4, "removed");

    world.disk.put(format!("{PACKAGES}/User/Preferences.sublime-settings"), "{\"font_size\": 12}");
    world.disk.put(format!("{PACKAGES}/User/notes.py"), "");
    check_twice(&mut editor);
    assert_eq!(loads(), 4, "files of a package are not watched, only its metadata");
}

#[test]
fn other_files_in_installed_packages_are_not_watched() {
    let world = World::new().with_installed_packages();
    let mut editor = world.editor();
    world.disk.put(format!("{INSTALLED_PACKAGES}/Theme.sublime-package.download"), "partial");
    editor.check_installed_packages();
    editor.check_installed_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1);
}

#[test]
fn a_change_of_ignored_packages_reloads_the_packages() {
    let world = World::new();
    let mut editor = world.editor();
    editor.check_ignored_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 1, "nothing changed");

    // Package Control disables a package: it sets and saves the user's preferences.
    world.resources.preferences.borrow_mut().push(json!({ "ignored_packages": ["Vintage", "Linter"] }));
    editor.reload_settings();
    editor.check_ignored_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    editor.check_ignored_packages();
    assert_eq!(world.resources.ui_loads.borrow().len(), 2, "loaded with it already");
}

#[test]
fn packages_location_reports_both_folders() {
    let world = World::new();
    let editor = world.editor();
    assert_eq!(editor.packages_location(), (PathBuf::from("/packages"), None));
}
