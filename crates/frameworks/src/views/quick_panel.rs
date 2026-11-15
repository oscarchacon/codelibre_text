//! The quick panel at the top of the window: Command Palette, Goto Anything and pickers.

use egui::text::{CCursor, CCursorRange};
use egui::{Id, Key, Modifiers, RichText, Sense};
use entities::minihtml::Span;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::QuickPanelViewModel;

use super::theme::Chrome;

pub fn show(
    ctx: &egui::Context,
    chrome: &Chrome,
    panel: &QuickPanelViewModel,
    events: &mut Vec<UiEvent>,
) {
    let input_id = Id::new("quick_panel_input");
    let event_modifiers = crate::input::current_modifiers(ctx);

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
        events.push(if panel.text_input {
            UiEvent::PluginInputPanelDone(crate::input::plugin_event(event_modifiers))
        } else if count > 0 && panel.wants_event {
            UiEvent::PluginQuickPanelAccepted {
                index: selected,
                event: crate::input::plugin_event(event_modifiers),
            }
        } else if count > 0 {
            UiEvent::QuickPanelAccepted(selected)
        } else {
            UiEvent::QuickPanelDismissed
        });
    }
    // An input handler's panel: Backspace in the empty field goes back to the previous argument.
    let can_go_back =
        panel.input.as_ref().is_some_and(|input| input.can_go_back) && panel.query.is_empty();
    if can_go_back
        && ctx.memory(|m| m.has_focus(input_id))
        && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace))
    {
        events.push(UiEvent::PluginPanelBack);
    }

    let screen = ctx.screen_rect();
    let width = (screen.width() * 0.5).clamp(420.0, 720.0);
    let area = egui::Area::new(Id::new("quick_panel"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(
            screen.center().x - width / 2.0,
            screen.top() + 44.0,
        ))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(chrome.panel_bg)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(width);
                    if let Some(input) = &panel.input {
                        breadcrumb(ui, chrome, &input.breadcrumb);
                    }
                    let mut query = panel.query.clone();
                    let input = egui::TextEdit::singleline(&mut query)
                        .id(input_id)
                        .hint_text(panel.placeholder.as_str())
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
                    if let Some((document, revision, anchor, head)) = panel.input_selection {
                        let seen = input_id.with("plugin_input_selection_revision");
                        let current = (document, revision);
                        if ui.data(|data| data.get_temp::<(u64, u64)>(seen)) != Some(current) {
                            ui.data_mut(|data| data.insert_temp(seen, current));
                            let mut state = input.state.clone();
                            state.cursor.set_char_range(Some(CCursorRange::two(
                                CCursor::new(anchor),
                                CCursor::new(head),
                            )));
                            state.store(ui.ctx(), input_id);
                        } else if let Some(range) = input.cursor_range {
                            let range = range.as_ccursor_range();
                            let selected = (range.secondary.index, range.primary.index);
                            if selected != (anchor, head) {
                                events.insert(
                                    0,
                                    UiEvent::InputPanelSelected {
                                        anchor: selected.0,
                                        head: selected.1,
                                    },
                                );
                            }
                        }
                    }
                    if query != panel.query {
                        events.push(UiEvent::QuickPanelQueryChanged(query));
                    }
                    if let Some(input) = panel
                        .input
                        .as_ref()
                        .filter(|input| !input.preview.is_empty())
                    {
                        ui.add_space(4.0);
                        for line in &input.preview {
                            if let Some(href) = detail_line(ui, chrome, line) {
                                events.push(UiEvent::QuickPanelLinkClicked(href));
                            }
                        }
                    }
                    if !panel.text_input {
                        ui.add_space(6.0);
                        results(
                            ui,
                            chrome,
                            panel,
                            selected,
                            moved,
                            screen.height() * 0.55,
                            events,
                        );
                    }
                });
        });

    // Clicking anywhere else dismisses the panel.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p))
    });
    if clicked_outside && !panel.keep_open {
        events.push(UiEvent::QuickPanelDismissed);
    }
}

/// The command and the arguments answered so far, above an input handler's field (as Sublime's
/// palette shows them). `>` separates them: the default fonts lack `›`.
pub fn breadcrumb(ui: &mut egui::Ui, chrome: &Chrome, crumbs: &[String]) {
    if crumbs.is_empty() {
        return;
    }
    ui.label(
        RichText::new(crumbs.join("  >  "))
            .color(chrome.fg_dim)
            .size(12.0),
    );
    ui.add_space(4.0);
}

/// The height a line of an item's details takes.
const DETAIL_LINE_HEIGHT: f32 = 14.0;

/// One line of an item's details, cut to the row's width. Returns the `href` of a link clicked
/// (a click anywhere else is the row's: it accepts the item).
pub fn detail_line(ui: &mut egui::Ui, chrome: &Chrome, line: &[Span]) -> Option<String> {
    let mut clicked = None;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), DETAIL_LINE_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for span in line.iter().filter(|s| !s.text.is_empty()) {
                let mut text = RichText::new(&span.text)
                    .size(11.5)
                    .color(if span.link.is_some() {
                        chrome.accent
                    } else {
                        chrome.fg_dim
                    });
                if span.bold {
                    text = text.strong();
                }
                if span.italic {
                    text = text.italics();
                }
                if span.monospace {
                    text = text.monospace();
                }
                let Some(href) = &span.link else {
                    ui.add(egui::Label::new(text).truncate());
                    continue;
                };
                let response = ui.add(
                    egui::Label::new(text.underline())
                        .truncate()
                        .sense(Sense::CLICK),
                );
                if response.clicked() {
                    clicked = Some(href.clone());
                }
                response.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        },
    );
    clicked
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
    // An Area lays out its contents within last frame's size, so after showing a single result
    // the list could never grow back. Give it room for `max_height` explicitly.
    let room = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), max_height),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| {
        result_list(
            ui,
            chrome,
            panel,
            selected,
            scroll_to_selected,
            max_height,
            events,
        )
    });
}

fn result_list(
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
                let height = 28.0 + DETAIL_LINE_HEIGHT * item.details.len() as f32;
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::CLICK);
                if is_selected || response.hovered() {
                    let fill = if is_selected {
                        chrome.hover_bg
                    } else {
                        chrome.hover_bg.gamma_multiply(0.5)
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                }
                let mut row = ui.new_child(
                    egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(10.0, 4.0))),
                );
                // Selectable labels sense clicks and, laid out after the row, take them from it:
                // a click on the text would select it instead of accepting the item.
                row.style_mut().interaction.selectable_labels = false;
                // The hint first, from the right, so the caption and detail get what is left and
                // are cut to one line each: the row has a fixed height.
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !item.hint.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.hint).size(12.0).color(chrome.fg_dim),
                            )
                            .extend(),
                        );
                        ui.add_space(12.0);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if !item.kind.is_empty() {
                            ui.label(
                                RichText::new(&item.kind)
                                    .size(12.0)
                                    .monospace()
                                    .color(chrome.panel_bg)
                                    .background_color(chrome.accent),
                            );
                        }
                        ui.vertical(|ui| {
                            let color = if is_selected {
                                chrome.accent
                            } else {
                                chrome.fg
                            };
                            let caption = RichText::new(&item.caption).size(14.0).color(color);
                            ui.add(
                                egui::Label::new(if panel.monospace {
                                    caption.monospace()
                                } else {
                                    caption
                                })
                                .truncate(),
                            );
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for line in &item.details {
                                if let Some(href) = detail_line(ui, chrome, line) {
                                    events.push(UiEvent::QuickPanelLinkClicked(href));
                                }
                            }
                        });
                    });
                });
                if is_selected && scroll_to_selected {
                    response.scroll_to_me(None);
                }
                if response.clicked() {
                    events.push(if panel.wants_event {
                        UiEvent::PluginQuickPanelAccepted {
                            index,
                            event: crate::input::plugin_event(crate::input::current_modifiers(
                                ui.ctx(),
                            )),
                        }
                    } else {
                        UiEvent::QuickPanelAccepted(index)
                    });
                }
            }
            if panel.items.is_empty() {
                ui.label(RichText::new(&panel.no_matches).color(chrome.fg_dim));
            }
        });
}
