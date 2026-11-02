use entities::menu::MenuItem;
use entities::settings::Settings;
use use_cases::Editor;

use crate::view_models::MenuEntryViewModel;

/// The main menu, in the interface language, with key bindings next to each command and check
/// marks for toggles. Plugin commands decide whether they show, are enabled and are checked.
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

fn entry(item: &MenuItem, editor: &Editor, settings: &Settings) -> Option<MenuEntryViewModel> {
    let state = editor.state();
    let plugin = item.command.as_ref().and_then(|c| editor.plugin_command_state(c));
    if plugin.is_some_and(|p| !p.visible) {
        return None;
    }
    let checked = item.checkbox.then(|| match plugin {
        Some(plugin) => plugin.checked,
        None => item.command.as_ref().map(|c| c.str_arg("setting")).is_some_and(|setting| settings.bool(setting, false)),
    });
    let caption = plugin.and_then(|p| p.description.clone()).filter(|_| item.caption.is_empty());
    Some(MenuEntryViewModel {
        caption: caption.unwrap_or_else(|| state.translations.tr(&item.caption).to_string()),
        separator: item.is_separator(),
        shortcut: item.command.as_ref().and_then(|c| state.keymap.binding_for(c)),
        checked,
        enabled: plugin.is_none_or(|p| p.enabled),
        command: item.command.clone(),
        children: item.children.iter().filter_map(|child| entry(child, editor, settings)).collect(),
    })
}
