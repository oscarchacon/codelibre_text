//! Modal questions and small windows: unsaved changes, About, package load errors.

use egui::RichText;
use entities::localization::Translations;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{AboutViewModel, DialogViewModel};
use use_cases::editor::CloseChoice;

pub fn show_close_dialog(ctx: &egui::Context, text: &Translations, dialog: &DialogViewModel, events: &mut Vec<UiEvent>) {
    let (body, choices): (String, Vec<(&str, CloseChoice)>) = match dialog {
        DialogViewModel::SaveChanges { document_name } => (
            text.tr_args("Save changes to {name} before closing?", &[("name", document_name)]),
            vec![
                (text.tr("Save"), CloseChoice::Save),
                (text.tr("Don't Save"), CloseChoice::DontSave),
                (text.tr("Cancel"), CloseChoice::Cancel),
            ],
        ),
        DialogViewModel::ExitWithUnsavedChanges => (
            text.tr("Some files have unsaved changes. Exit anyway?").to_string(),
            vec![(text.tr("Exit"), CloseChoice::DontSave), (text.tr("Cancel"), CloseChoice::Cancel)],
        ),
    };
    egui::Window::new(text.tr("Unsaved changes"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(body);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, choice) in &choices {
                    if ui.button(*label).clicked() {
                        events.push(UiEvent::CloseDialogAnswered(*choice));
                    }
                }
            });
        });
}

pub fn show_about(ctx: &egui::Context, text: &Translations, about: &AboutViewModel, events: &mut Vec<UiEvent>) {
    let mut open = true;
    egui::Window::new(text.tr("About CodeLibre Text"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.heading("CodeLibre Text");
            ui.label(text.tr_args("Version {version}", &[("version", &about.version)]));
            ui.label(text.tr(
                "An open-source native editor written in Rust,
compatible with Sublime Text packages (syntaxes, color schemes,
key bindings, menus, command palette and settings).",
            ));
            ui.add_space(6.0);
            ui.label(text.tr_args("Packages: {path}", &[("path", &about.packages)]));
            if let Some(sublime) = &about.sublime_packages {
                ui.label(text.tr_args("Sublime Text packages: {path}", &[("path", sublime)]));
            }
        });
    if !open {
        events.push(UiEvent::AboutClosed);
    }
}

pub fn show_load_errors(ctx: &egui::Context, text: &Translations, errors: &[String], events: &mut Vec<UiEvent>) {
    let mut open = true;
    egui::Window::new(text.tr("Package Load Errors"))
        .open(&mut open)
        .default_size([640.0, 360.0])
        .show(ctx, |ui| {
            if errors.is_empty() {
                ui.label(text.tr("No errors."));
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
