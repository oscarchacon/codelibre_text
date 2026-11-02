//! Tab bar. A dirty tab shows a dot that turns into a close button on hover, as in Sublime.
//!
//! Widgets here use `Sense::CLICK`, not `Sense::click()`: the latter is keyboard-focusable, and
//! focusable chrome would steal arrow keys / Enter from the text.

use egui::{Align2, FontId, Rect, Sense, Ui};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::TabViewModel;

use super::theme::Chrome;

const HEIGHT: f32 = 30.0;

pub fn show(ui: &mut Ui, chrome: &Chrome, tabs: &[TabViewModel], events: &mut Vec<UiEvent>) {
    egui::ScrollArea::horizontal()
        .id_salt("tab_bar")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                for (index, tab) in tabs.iter().enumerate() {
                    if let Some(event) = tab_button(ui, chrome, index, tab) {
                        events.push(event);
                    }
                }
                // Double click on the empty strip creates a new file, as in Sublime.
                let rest = ui.available_rect_before_wrap();
                let rest = Rect::from_min_size(rest.min, egui::vec2(rest.width().max(40.0), HEIGHT));
                if ui.interact(rest, ui.id().with("tab_strip"), Sense::CLICK).double_clicked() {
                    events.push(UiEvent::NewTab);
                }
            });
        });
}

fn tab_button(ui: &mut Ui, chrome: &Chrome, index: usize, tab: &TabViewModel) -> Option<UiEvent> {
    let font = FontId::proportional(13.0);
    let text_width = ui.fonts(|f| f.layout_no_wrap(tab.title.clone(), font.clone(), chrome.fg).size().x);
    let width = (text_width + 48.0).clamp(90.0, 260.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), Sense::CLICK);

    let background = if tab.active {
        chrome.editor_bg
    } else if response.hovered() {
        chrome.hover_bg
    } else {
        chrome.tab_bg
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, background);
    if tab.active {
        painter.line_segment(
            [rect.left_top() + egui::vec2(4.0, 0.5), rect.right_top() + egui::vec2(-4.0, 0.5)],
            egui::Stroke::new(2.0_f32, chrome.accent),
        );
    }
    let color = if tab.active { chrome.fg } else { chrome.fg_dim };
    painter.text(egui::pos2(rect.left() + 12.0, rect.center().y), Align2::LEFT_CENTER, &tab.title, font, color);

    let close_rect = Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), egui::vec2(16.0, 16.0));
    let close = ui.interact(close_rect, ui.id().with(("close_tab", index)), Sense::CLICK);
    if close.hovered() {
        ui.painter().rect_filled(close_rect, 3.0, chrome.hover_bg);
    }
    let show_close = close.hovered() || (!tab.dirty && (response.hovered() || tab.active));
    if show_close {
        ui.painter()
            .text(close_rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(15.0), color);
    } else if tab.dirty {
        ui.painter().circle_filled(close_rect.center(), 3.5, color);
    }

    let event = if close.clicked() || response.middle_clicked() {
        Some(UiEvent::CloseTab(index))
    } else if response.clicked() {
        Some(UiEvent::ActivateTab(index))
    } else {
        None
    };
    response.on_hover_text(&tab.tooltip);
    event
}
