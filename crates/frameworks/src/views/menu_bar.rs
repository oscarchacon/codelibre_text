//! Main menu, built from `Main.sublime-menu` (merged across packages).

use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::MenuEntryViewModel;

pub fn show(ui: &mut egui::Ui, menu: &[MenuEntryViewModel], events: &mut Vec<UiEvent>) {
    egui::menu::bar(ui, |ui| {
        for top in menu {
            ui.menu_button(&top.caption, |ui| {
                ui.set_min_width(220.0);
                entries(ui, &top.children, events);
            });
        }
    });
}

fn entries(ui: &mut egui::Ui, items: &[MenuEntryViewModel], events: &mut Vec<UiEvent>) {
    for entry in items {
        if entry.separator {
            ui.separator();
            continue;
        }
        if !entry.children.is_empty() {
            ui.menu_button(&entry.caption, |ui| {
                ui.set_min_width(200.0);
                entries(ui, &entry.children, events);
            });
            continue;
        }
        let Some(command) = &entry.command else {
            ui.add_enabled(false, egui::Button::new(&entry.caption));
            continue;
        };
        let caption = match entry.checked {
            Some(true) => format!("✔ {}", entry.caption),
            Some(false) => format!("   {}", entry.caption),
            None => entry.caption.clone(),
        };
        let mut button = egui::Button::new(caption);
        if let Some(shortcut) = &entry.shortcut {
            button = button.shortcut_text(shortcut);
        }
        if ui.add(button).clicked() {
            events.push(UiEvent::RunCommand(command.clone()));
            ui.close_menu();
        }
    }
}
