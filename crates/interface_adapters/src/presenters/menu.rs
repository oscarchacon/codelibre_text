use entities::keymap::Keymap;
use entities::menu::MenuItem;
use entities::settings::Settings;
use use_cases::Editor;

use crate::view_models::MenuEntryViewModel;

/// The main menu with key bindings next to each command and check marks for toggles.
pub fn present(editor: &Editor) -> Vec<MenuEntryViewModel> {
    let state = editor.state();
    let settings = editor.global_settings();
    state
        .main_menu
        .iter()
        .filter(|item| !item.children.is_empty())
        .map(|item| entry(item, &state.keymap, &settings))
        .collect()
}

fn entry(item: &MenuItem, keymap: &Keymap, settings: &Settings) -> MenuEntryViewModel {
    let checked = item.checkbox.then(|| {
        item.command
            .as_ref()
            .map(|c| c.str_arg("setting"))
            .is_some_and(|setting| settings.bool(setting, false))
    });
    MenuEntryViewModel {
        caption: item.caption.clone(),
        separator: item.is_separator(),
        shortcut: item.command.as_ref().and_then(|c| keymap.binding_for(c)),
        checked,
        command: item.command.clone(),
        children: item.children.iter().map(|child| entry(child, keymap, settings)).collect(),
    }
}
