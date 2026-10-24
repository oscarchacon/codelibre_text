//! Side bar: OPEN FILES and the FOLDERS tree.

use egui::{RichText, Sense, Ui};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{SideBarViewModel, TreeRowKind};

use super::theme::{mix, Chrome};

const ROW_HEIGHT: f32 = 22.0;

pub fn show(ui: &mut Ui, chrome: &Chrome, side_bar: &SideBarViewModel, events: &mut Vec<UiEvent>) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(6.0);

        if !side_bar.open_files.is_empty() {
            section_header(ui, chrome, "OPEN FILES");
            for (index, file) in side_bar.open_files.iter().enumerate() {
                let response = row(ui, chrome, 0, None, &file.name, file.active);
                if file.dirty {
                    let center = egui::pos2(response.rect.left() + 16.0, response.rect.center().y);
                    ui.painter().circle_filled(center, 3.0, chrome.fg_dim);
                }
                if response.clicked() {
                    events.push(UiEvent::ActivateTab(index));
                }
                if response.middle_clicked() {
                    events.push(UiEvent::CloseTab(index));
                }
            }
            ui.add_space(8.0);
        }

        if !side_bar.folders.is_empty() {
            section_header(ui, chrome, "FOLDERS");
            for entry in &side_bar.folders {
                let folder = match entry.kind {
                    TreeRowKind::Folder { expanded, .. } => Some(expanded),
                    TreeRowKind::File => None,
                };
                let response = row(ui, chrome, entry.depth, folder, &entry.name, entry.active);
                if response.clicked() {
                    events.push(match entry.kind {
                        TreeRowKind::Folder { .. } => UiEvent::ToggleFolder(entry.path.clone()),
                        TreeRowKind::File => UiEvent::OpenFile(entry.path.clone()),
                    });
                }
                if let TreeRowKind::Folder { is_root: true, .. } = entry.kind {
                    response.context_menu(|ui| {
                        if ui.button("Remove Folder from Project").clicked() {
                            events.push(UiEvent::RemoveFolder(entry.path.clone()));
                            ui.close_menu();
                        }
                    });
                }
            }
        }
    });
}

fn section_header(ui: &mut Ui, chrome: &Chrome, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(RichText::new(title).size(10.5).color(chrome.fg_dim).strong());
    });
    ui.add_space(4.0);
}

/// A full-width clickable row. `folder` is `Some(expanded)` for folders.
fn row(ui: &mut Ui, chrome: &Chrome, depth: usize, folder: Option<bool>, label: &str, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::CLICK);
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 0.0, chrome.hover_bg);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, chrome.hover_bg.gamma_multiply(0.6));
    }

    let x = rect.left() + 10.0 + depth as f32 * 14.0;
    if let Some(expanded) = folder {
        // Disclosure triangle (the default fonts have no ▸/▾ glyphs).
        let c = egui::pos2(x + 4.0, rect.center().y);
        let points = if expanded {
            vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)]
        } else {
            vec![c + egui::vec2(-2.0, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-2.0, 4.0)]
        };
        painter.add(egui::Shape::convex_polygon(points, chrome.fg_dim, egui::Stroke::NONE));
    }
    let color = if active { chrome.fg } else { mix(chrome.fg, chrome.fg_dim, 0.35) };
    painter.text(
        egui::pos2(x + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    response
}
