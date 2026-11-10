use entities::command::Command;
use entities::menu::MenuItem;
use entities::settings::Settings;
use use_cases::Editor;

use crate::view_models::{ContextMenuViewModel, MacMenuViewModel, MenuEntryViewModel};
use crate::view_state::ViewState;

/// The main menu, in the interface language, with key bindings next to each command and check
/// marks for toggles. Plugin commands decide whether they show, are enabled and are checked.
/// Submenus nothing was added to (Preferences > Package Settings without packages that have
/// settings) are left out, and so are the separators that would end up at an edge or doubled.
pub fn present(editor: &Editor) -> Vec<MenuEntryViewModel> {
    let settings = editor.global_settings();
    editor
        .state()
        .main_menu
        .iter()
        .filter(|item| !item.children.is_empty())
        .filter_map(|item| entry(item, editor, &settings))
        .collect()
}

/// The context menu open in this window, with its arguments filled for what it was opened on.
/// Entries hidden by their plugin are left out, as in the main menu.
pub fn present_context_menu(editor: &Editor, view: &ViewState) -> Option<ContextMenuViewModel> {
    let target = view.context_menu.clone()?;
    let entries = entries(&editor.context_menu_items(&target), editor, &editor.global_settings());
    Some(ContextMenuViewModel { target, entries })
}

/// A menu's entries; `{"command": "$build_systems"}` becomes one entry per build system.
fn entries(items: &[MenuItem], editor: &Editor, settings: &Settings) -> Vec<MenuEntryViewModel> {
    let mut entries = Vec::new();
    for item in items {
        if item.command.as_ref().is_some_and(|c| c.name == "$build_systems") {
            entries.extend(build_system_entries(editor));
        } else {
            entries.extend(entry(item, editor, settings));
        }
    }
    tidy(entries)
}

/// Tools > Build System: `set_build_system` with each build system, the one set checked.
fn build_system_entries(editor: &Editor) -> Vec<MenuEntryViewModel> {
    let set = editor.build_system_set();
    let state = editor.state();
    editor
        .menu_build_systems()
        .iter()
        .map(|build| {
            let command = Command::with_args("set_build_system", serde_json::json!({ "file": build.resource }));
            MenuEntryViewModel {
                caption: build.caption.clone(),
                id: None,
                separator: false,
                shortcut: state.keymap.binding_for(&command),
                key: state.keymap.key_for(&command).cloned(),
                checked: Some(set == Some(build.resource.as_str())),
                enabled: true,
                command: Some(command),
                children: Vec::new(),
            }
        })
        .collect()
}

fn entry(item: &MenuItem, editor: &Editor, settings: &Settings) -> Option<MenuEntryViewModel> {
    let state = editor.state();
    let plugin = item.command.as_ref().and_then(|c| editor.plugin_command_state(c));
    if plugin.is_some_and(|p| !p.visible) {
        return None;
    }
    let checked = item.checkbox.then(|| match (plugin, &item.command) {
        (Some(plugin), _) => plugin.checked,
        (None, Some(c)) if c.name == "set_build_system" => editor.build_system_set().unwrap_or_default() == c.str_arg("file"),
        (None, Some(c)) if c.name == "set_max_columns" => editor.max_columns() == c.usize_arg("columns", 0),
        (None, command) => command.as_ref().map(|c| c.str_arg("setting")).is_some_and(|setting| settings.bool(setting, false)),
    });
    let caption = plugin.and_then(|p| p.description.clone()).filter(|_| item.caption.is_empty());
    let children = entries(&item.children, editor, settings);
    if item.command.is_none() && !item.is_separator() && children.is_empty() {
        return None;
    }
    Some(MenuEntryViewModel {
        caption: caption.unwrap_or_else(|| state.translations.tr(&item.caption).to_string()),
        id: item.id.clone(),
        separator: item.is_separator(),
        shortcut: item.command.as_ref().and_then(|c| state.keymap.binding_for(c)),
        key: item.command.as_ref().and_then(|c| state.keymap.key_for(c)).cloned(),
        checked,
        enabled: plugin.is_none_or(|p| p.enabled),
        command: item.command.clone(),
        children,
    })
}

/// The main menu laid out as macOS apps do (and Sublime Text there): About, Preferences and Quit
/// (`exit`, captioned "Quit CodeLibre Text") move to the application menu.
pub fn present_for_macos(editor: &Editor) -> MacMenuViewModel {
    let mut menus = present(editor);
    let preferences = menus
        .iter()
        .position(|m| m.id.as_deref() == Some("preferences"))
        .map(|index| menus.remove(index));
    let about = take_command(&mut menus, "show_about_window");
    let quit = take_command(&mut menus, "exit").map(|quit| MenuEntryViewModel {
        caption: editor.tr("Quit CodeLibre Text").to_string(),
        ..quit
    });
    menus.retain(|m| !m.children.is_empty());
    MacMenuViewModel {
        app_name: "CodeLibre Text".to_string(),
        about,
        preferences,
        services: editor.tr("Services").to_string(),
        hide: editor.tr("Hide CodeLibre Text").to_string(),
        hide_others: editor.tr("Hide Others").to_string(),
        show_all: editor.tr("Show All").to_string(),
        quit,
        menus,
    }
}

/// Removes the first entry of a top menu that runs `command`.
fn take_command(menus: &mut [MenuEntryViewModel], command: &str) -> Option<MenuEntryViewModel> {
    for menu in menus {
        let found = menu.children.iter().position(|e| e.command.as_ref().is_some_and(|c| c.name == command));
        if let Some(index) = found {
            let entry = menu.children.remove(index);
            menu.children = tidy(std::mem::take(&mut menu.children));
            return Some(entry);
        }
    }
    None
}

/// Drops separators at the start or end and those right after another one.
fn tidy(entries: Vec<MenuEntryViewModel>) -> Vec<MenuEntryViewModel> {
    let mut tidy: Vec<MenuEntryViewModel> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.separator && tidy.last().is_none_or(|last| last.separator) {
            continue;
        }
        tidy.push(entry);
    }
    if tidy.last().is_some_and(|last| last.separator) {
        tidy.pop();
    }
    tidy
}
