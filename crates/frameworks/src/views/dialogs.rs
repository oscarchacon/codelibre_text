//! Modal questions and small windows: unsaved changes, About, package load errors.

use egui::RichText;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{AboutViewModel, DialogViewModel};
use use_cases::editor::CloseChoice;

pub fn show_close_dialog(ctx: &egui::Context, dialog: &DialogViewModel, events: &mut Vec<UiEvent>) {
    let (body, choices): (String, &[(&str, CloseChoice)]) = match dialog {
        DialogViewModel::SaveChanges { document_name } => (
            format!("Save changes to {document_name} before closing?"),
            &[("Save", CloseChoice::Save), ("Don't Save", CloseChoice::DontSave), ("Cancel", CloseChoice::Cancel)],
        ),
        DialogViewModel::ExitWithUnsavedChanges => (
            "Some files have unsaved changes. Exit anyway?".into(),
            &[("Exit", CloseChoice::DontSave), ("Cancel", CloseChoice::Cancel)],
        ),
    };
    egui::Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(body);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, choice) in choices {
                    if ui.button(*label).clicked() {
                        events.push(UiEvent::CloseDialogAnswered(*choice));
                    }
                }
            });
        });
}

pub fn show_about(ctx: &egui::Context, about: &AboutViewModel, events: &mut Vec<UiEvent>) {
    let mut open = true;
    egui::Window::new("About CodeLibre Text")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.heading("CodeLibre Text");
            ui.label(format!("Version {}", about.version));
            ui.label("An open-source native editor written in Rust,");
            ui.label("compatible with Sublime Text packages (syntaxes, color schemes,");
            ui.label("key bindings, menus, command palette and settings).");
            ui.add_space(6.0);
            ui.label(format!("Packages: {}", about.packages));
            if let Some(sublime) = &about.sublime_packages {
                ui.label(format!("Sublime Text packages: {sublime}"));
            }
        });
    if !open {
        events.push(UiEvent::AboutClosed);
    }
}

pub fn show_load_errors(ctx: &egui::Context, errors: &[String], events: &mut Vec<UiEvent>) {
    let mut open = true;
    egui::Window::new("Package Load Errors")
        .open(&mut open)
        .default_size([640.0, 360.0])
        .show(ctx, |ui| {
            if errors.is_empty() {
                ui.label("No errors.");
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for error in errors {
                    ui.label(RichText::new(error).monospace().size(11.0));
                    ui.separator();
                }
            });
        });
    if !open {
        events.push(UiEvent::LoadErrorsClosed);
    }
}
