//! Tests for `gateways::sublime_resources` (settings, key bindings, menus and palette files).

use std::fs;
use std::path::Path;

use entities::command::Command;
use entities::keymap::KeyCombo;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use serde_json::{json, Map};
use tempfile::TempDir;
use use_cases::gateways::{platform_name, PackagePaths, ResourceGateway, UiResources};

/// A temporary `Packages` folder (and an optional Sublime Text `Packages` folder).
struct Packages {
    dir: TempDir,
}

impl Packages {
    fn new() -> Self {
        Self { dir: tempfile::tempdir().unwrap() }
    }

    fn ours(&self) -> std::path::PathBuf {
        self.dir.path().join("Packages")
    }

    fn sublimes(&self) -> std::path::PathBuf {
        self.dir.path().join("Sublime").join("Packages")
    }

    /// Writes `Packages/<package>/<file>`.
    fn file(&self, package: &str, file: &str, contents: &str) -> &Self {
        write(&self.ours().join(package).join(file), contents);
        self
    }

    fn sublime_file(&self, package: &str, file: &str, contents: &str) -> &Self {
        write(&self.sublimes().join(package).join(file), contents);
        self
    }

    fn gateway(&self) -> SublimeResourceGateway {
        let sublime = self.sublimes().is_dir().then(|| self.sublimes());
        SublimeResourceGateway::new(PackagePaths { packages: self.ours(), sublime_packages: sublime })
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn ui(gateway: &SublimeResourceGateway, include_sublime: bool) -> (UiResources, Vec<String>) {
    let mut errors = Vec::new();
    let resources = gateway.load_ui_resources(include_sublime, &mut errors);
    (resources, errors)
}

fn bound_command(resources: &UiResources, key: &str) -> Option<String> {
    let combo = KeyCombo::parse(key).unwrap();
    resources
        .keymap
        .bindings
        .iter()
        .rev()
        .find(|b| b.keys == vec![combo.clone()])
        .map(|b| b.command.name.clone())
}

// ----- settings ------------------------------------------------------------------------------------

#[test]
fn creating_the_gateway_creates_the_user_package() {
    let packages = Packages::new();
    packages.gateway();
    assert!(packages.ours().join("User").is_dir());
}

#[test]
fn builtin_defaults_are_loaded_without_any_package() {
    let packages = Packages::new();
    let mut errors = Vec::new();
    let settings = packages.gateway().load_settings(&mut errors).global();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(settings.str("color_scheme"), Some("Monokai.sublime-color-scheme"));
    assert_eq!(settings.usize("tab_size", 0), 4);
}

#[test]
fn user_preferences_override_package_preferences() {
    let packages = Packages::new();
    packages
        .file("Alpha", "Preferences.sublime-settings", r#"{ "tab_size": 3, "rulers": [80] }"#)
        .file("User", "Preferences.sublime-settings", "{ // mine\n \"tab_size\": 2, }");
    let settings = packages.gateway().load_settings(&mut Vec::new()).global();
    assert_eq!(settings.usize("tab_size", 0), 2);
    assert_eq!(settings.numbers("rulers"), vec![80]);
}

#[test]
fn platform_preferences_are_a_preferences_layer() {
    let packages = Packages::new();
    let file = format!("Preferences ({}).sublime-settings", platform_name());
    packages.file("User", &file, r#"{ "font_size": 15 }"#);
    let settings = packages.gateway().load_settings(&mut Vec::new()).global();
    assert_eq!(settings.f32("font_size", 0.0), 15.0);
}

#[test]
fn syntax_settings_files_only_apply_to_their_syntax() {
    let packages = Packages::new();
    packages.file("User", "Python.sublime-settings", r#"{ "tab_size": 8 }"#);
    let layers = packages.gateway().load_settings(&mut Vec::new());
    assert_eq!(layers.for_syntax("Python").usize("tab_size", 0), 8);
    assert_eq!(layers.for_syntax("Rust").usize("tab_size", 0), 4);
}

#[test]
fn broken_settings_files_are_reported_with_their_path() {
    let packages = Packages::new();
    packages.file("User", "Preferences.sublime-settings", "{ \"tab_size\": }");
    let mut errors = Vec::new();
    let settings = packages.gateway().load_settings(&mut errors).global();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Preferences.sublime-settings"));
    assert_eq!(settings.usize("tab_size", 0), 4, "defaults still apply");
}

#[test]
fn sublime_text_settings_are_read_unless_disabled_by_the_user() {
    let packages = Packages::new();
    packages.sublime_file("User", "Preferences.sublime-settings", r#"{ "tab_size": 6 }"#);
    assert_eq!(packages.gateway().load_settings(&mut Vec::new()).global().usize("tab_size", 0), 6);

    packages.file("User", "Preferences.sublime-settings", r#"{ "import_sublime_packages": false }"#);
    assert_eq!(packages.gateway().load_settings(&mut Vec::new()).global().usize("tab_size", 0), 4);
}

#[test]
fn user_settings_can_be_written_and_read_back() {
    let packages = Packages::new();
    let gateway = packages.gateway();
    assert!(gateway.read_user_settings().is_empty());

    let mut values = Map::new();
    values.insert("font_size".into(), json!(13));
    gateway.write_user_settings(&values).unwrap();
    assert_eq!(gateway.read_user_settings()["font_size"], json!(13));
    assert_eq!(gateway.load_settings(&mut Vec::new()).global().f32("font_size", 0.0), 13.0);
}

#[test]
fn default_resources_for_edit_settings() {
    let gateway = Packages::new().gateway();
    assert!(gateway.default_resource("Preferences.sublime-settings").unwrap().contains("color_scheme"));
    assert!(gateway.default_resource("Default (Linux).sublime-keymap").unwrap().contains("show_overlay"));
    assert!(gateway.default_resource("Rust.sublime-settings").is_none());
}

#[test]
fn paths_are_the_ones_given() {
    let packages = Packages::new();
    assert_eq!(packages.gateway().paths().packages, packages.ours());
}

// ----- key bindings --------------------------------------------------------------------------------

#[test]
fn builtin_key_bindings_are_loaded() {
    let (resources, errors) = ui(&Packages::new().gateway(), false);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(bound_command(&resources, "ctrl+shift+p").as_deref(), Some("show_overlay"));
    assert_eq!(bound_command(&resources, "ctrl+s").as_deref(), Some("save"));
}

#[test]
fn package_key_bindings_override_the_builtin_ones() {
    let packages = Packages::new();
    packages
        .file("MyPackage", "Default.sublime-keymap", r#"[{ "keys": ["ctrl+s"], "command": "save_all" }]"#)
        .file("User", &format!("Default ({}).sublime-keymap", platform_name()), r#"[{ "keys": ["ctrl+q"], "command": "exit" }]"#);
    let (resources, _) = ui(&packages.gateway(), false);
    assert_eq!(bound_command(&resources, "ctrl+s").as_deref(), Some("save_all"));
    assert_eq!(bound_command(&resources, "ctrl+q").as_deref(), Some("exit"));
}

#[test]
fn bindings_keep_arguments_and_contexts() {
    let packages = Packages::new();
    packages.file(
        "User",
        "Default.sublime-keymap",
        r#"[{ "keys": ["alt+x"], "command": "insert", "args": { "characters": "x" },
              "context": [{ "key": "selection_empty" }] }]"#,
    );
    let (resources, _) = ui(&packages.gateway(), false);
    let binding = resources.keymap.bindings.last().unwrap();
    assert_eq!(binding.command, Command::with_args("insert", json!({ "characters": "x" })));
    assert_eq!(binding.context.len(), 1);
}

#[test]
fn bindings_with_invalid_keys_are_skipped() {
    let packages = Packages::new();
    packages.file("User", "Default.sublime-keymap", r#"[{ "keys": ["hyper+x"], "command": "a" }, { "keys": ["f5"] }]"#);
    let (with_bad, _) = ui(&packages.gateway(), false);
    let (builtin, _) = ui(&Packages::new().gateway(), false);
    assert_eq!(with_bad.keymap.bindings.len(), builtin.keymap.bindings.len());
}

#[test]
fn sublime_texts_own_default_keymap_is_ignored() {
    let packages = Packages::new();
    packages.sublime_file("Default", "Default.sublime-keymap", r#"[{ "keys": ["ctrl+s"], "command": "native_save" }]"#);
    let (resources, _) = ui(&packages.gateway(), true);
    assert_eq!(bound_command(&resources, "ctrl+s").as_deref(), Some("save"));
}

#[test]
fn broken_keymaps_are_reported() {
    let packages = Packages::new();
    packages.file("User", "Default.sublime-keymap", "[{ broken");
    let (_, errors) = ui(&packages.gateway(), false);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Default.sublime-keymap"));
}

// ----- menus and the command palette ---------------------------------------------------------------

#[test]
fn the_builtin_main_menu_is_loaded() {
    let (resources, _) = ui(&Packages::new().gateway(), false);
    let captions: Vec<&str> = resources.main_menu.iter().map(|m| m.caption.as_str()).collect();
    assert_eq!(captions[0], "File");
    let view = resources.main_menu.iter().find(|m| m.caption == "View").unwrap();
    let word_wrap = view.children.iter().find(|c| c.caption == "Word Wrap").unwrap();
    assert!(word_wrap.checkbox);
    assert_eq!(word_wrap.command.as_ref().unwrap().str_arg("setting"), "word_wrap");
}

#[test]
fn package_menus_merge_into_existing_menus_by_id() {
    let packages = Packages::new();
    packages.file(
        "MyPackage",
        "Main.sublime-menu",
        r#"[{ "id": "tools", "children": [{ "caption": "&My Tool", "command": "my_tool" }, { "command": "no_caption" }] }]"#,
    );
    let (resources, _) = ui(&packages.gateway(), false);
    let tools = resources.main_menu.iter().find(|m| m.id.as_deref() == Some("tools")).unwrap();
    let captions: Vec<&str> = tools.children.iter().map(|c| c.caption.as_str()).collect();
    assert!(captions.ends_with(&["My Tool", "no_caption"]), "{captions:?}");
    assert_eq!(resources.main_menu.iter().filter(|m| m.id.as_deref() == Some("tools")).count(), 1);
}

#[test]
fn palette_entries_from_every_package_are_added() {
    let packages = Packages::new();
    packages.file(
        "MyPackage",
        "MyPackage.sublime-commands",
        r#"[{ "caption": "My: Thing", "command": "my_thing", "args": { "x": 1 } }, { "caption": "No command" }]"#,
    );
    let (resources, _) = ui(&packages.gateway(), false);
    let mine: Vec<_> = resources.palette.iter().filter(|p| p.caption.starts_with("My:")).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].command, Command::with_args("my_thing", json!({ "x": 1 })));
    assert!(resources.palette.iter().any(|p| p.caption == "File: Save"));
    assert!(!resources.palette.iter().any(|p| p.caption == "No command"));
}

#[test]
fn sublime_text_packages_add_palette_entries_only_when_included() {
    let packages = Packages::new();
    packages.sublime_file("Git", "Git.sublime-commands", r#"[{ "caption": "Git: Status", "command": "git_status" }]"#);
    let has_git = |include| ui(&packages.gateway(), include).0.palette.iter().any(|p| p.caption == "Git: Status");
    assert!(has_git(true));
    assert!(!has_git(false));
}
