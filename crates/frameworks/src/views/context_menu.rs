//! Right-click menus (`Context`, `Side Bar`, `Tab Context.sublime-menu`), drawn as egui context
//! menus. While one is open its view reports what it was opened on every frame; the entries come a
//! frame later, once the plugins were asked about them (until then the menu is empty).

use entities::menu::ContextMenuTarget;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::ContextMenuViewModel;

use super::menu_bar;

/// The context menu of `response` (opened by a right click on it), for `target`.
pub fn attach(response: &egui::Response, target: ContextMenuTarget, menu: Option<&ContextMenuViewModel>, events: &mut Vec<UiEvent>) {
    response.context_menu(|ui| {
        let ours = menu.filter(|m| m.target == target);
        events.push(UiEvent::ContextMenu(Some(target)));
        let Some(menu) = ours else { return };
        if menu.entries.is_empty() {
            ui.close_menu();
            return;
        }
        ui.set_min_width(200.0);
        menu_bar::entries(ui, &menu.entries, events);
    });
}

/// After the views: no context menu is open when none said so this frame.
pub fn report_closed(events: &mut Vec<UiEvent>) {
    if !events.iter().any(|e| matches!(e, UiEvent::ContextMenu(Some(_)))) {
        events.push(UiEvent::ContextMenu(None));
    }
}
