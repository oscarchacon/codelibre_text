//! The quick panel at the top of the window: Command Palette, Goto Anything and pickers.

use egui::text::{CCursor, CCursorRange};
use egui::{Id, Key, Modifiers, RichText, Sense};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::QuickPanelViewModel;

use super::theme::Chrome;

pub fn show(ctx: &egui::Context, chrome: &Chrome, panel: &QuickPanelViewModel, events: &mut Vec<UiEvent>) {
    let input_id = Id::new("quick_panel_input");

    // Keyboard navigation of the results.
    let (up, down, enter, page_up, page_down) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::PageUp),
            i.consume_key(Modifiers::NONE, Key::PageDown),
        )
    });
    let count = panel.items.len();
    let mut selected = panel.selected;
    if count > 0 {
        let last = count - 1;
        if up {
            selected = if selected == 0 { last } else { selected - 1 };
        }
        if down {
            selected = if selected >= last { 0 } else { selected + 1 };
        }
        if page_up {
            selected = selected.saturating_sub(10);
        }
        if page_down {
            selected = (selected + 10).min(last);
        }
    }
    let moved = selected != panel.selected;
    if moved {
        events.push(UiEvent::QuickPanelHighlighted(selected));
    }
    if enter {
        events.push(if count > 0 { UiEvent::QuickPanelAccepted(selected) } else { UiEvent::QuickPanelDismissed });
    }

    let screen = ctx.screen_rect();
    let width = (screen.width() * 0.5).clamp(420.0, 720.0);
    let area = egui::Area::new(Id::new("quick_panel"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.top() + 44.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(chrome.panel_bg)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(width);
                    let mut query = panel.query.clone();
                    let input = egui::TextEdit::singleline(&mut query)
                        .id(input_id)
                        .hint_text(panel.placeholder)
                        .desired_width(f32::INFINITY)
                        .font(egui::FontId::proportional(15.0))
                        .margin(egui::Margin::symmetric(8, 6))
                        .show(ui);
                    if panel.take_focus {
                        input.response.request_focus();
                        let mut state = input.state.clone();
                        let end = CCursor::new(query.chars().count());
                        state.cursor.set_char_range(Some(CCursorRange::one(end)));
                        state.store(ui.ctx(), input_id);
                    }
                    if query != panel.query {
                        events.push(UiEvent::QuickPanelQueryChanged(query));
                    }
                    ui.add_space(6.0);
                    results(ui, chrome, panel, selected, moved, screen.height() * 0.55, events);
                });
        });

    // Clicking anywhere else dismisses the panel.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p))
    });
    if clicked_outside {
        events.push(UiEvent::QuickPanelDismissed);
    }
}

fn results(
    ui: &mut egui::Ui,
    chrome: &Chrome,
    panel: &QuickPanelViewModel,
    selected: usize,
    scroll_to_selected: bool,
    max_height: f32,
    events: &mut Vec<UiEvent>,
) {
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, item) in panel.items.iter().enumerate() {
                let is_selected = index == selected;
                let height = if item.detail.is_empty() { 28.0 } else { 42.0 };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::CLICK);
                if is_selected || response.hovered() {
                    let fill = if is_selected { chrome.hover_bg } else { chrome.hover_bg.gamma_multiply(0.5) };
                    ui.painter().rect_filled(rect, 4.0, fill);
                }
                let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(10.0, 4.0))));
                row.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let color = if is_selected { chrome.accent } else { chrome.fg };
                        ui.label(RichText::new(&item.caption).size(14.0).color(color));
                        if !item.detail.is_empty() {
                            ui.label(RichText::new(&item.detail).size(11.5).color(chrome.fg_dim));
                        }
                    });
                    if !item.hint.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(&item.hint).size(12.0).color(chrome.fg_dim));
                        });
                    }
                });
                if is_selected && scroll_to_selected {
                    response.scroll_to_me(None);
                }
                if response.clicked() {
                    events.push(UiEvent::QuickPanelAccepted(index));
                }
            }
            if panel.items.is_empty() {
                ui.label(RichText::new("No matches").color(chrome.fg_dim));
            }
        });
}
