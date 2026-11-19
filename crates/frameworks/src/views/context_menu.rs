//! Right-click menus (`Context`, `Side Bar`, `Tab Context`, `Widget Context.sublime-menu`), drawn
//! as egui context menus. While one is open its view reports what it was opened on every frame; the
//! entries come a frame later, once the plugins were asked about them (until then the menu is
//! empty).

use egui::text::CCursorRange;
use egui::text_edit::TextEditState;
use entities::menu::ContextMenuTarget;
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{ContextMenuViewModel, PopupMenuViewModel};

use super::menu_bar;

/// The context menu of `response` (opened by a right click on it), for `target`.
pub fn attach(
    response: &egui::Response,
    target: ContextMenuTarget,
    menu: Option<&ContextMenuViewModel>,
    events: &mut Vec<UiEvent>,
) {
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

/// The context menu of a one-line field (`Widget Context.sublime-menu`), `field` being its widget id.
pub fn attach_to_field(
    response: &egui::Response,
    field: &str,
    menu: Option<&ContextMenuViewModel>,
    events: &mut Vec<UiEvent>,
) {
    let target = ContextMenuTarget::Widget {
        field: field.to_string(),
    };
    attach(response, target, menu, events);
}

/// egui's text fields put the caret where any mouse button goes down, the right one's too, which
/// would lose the selection a context menu's Copy or Cut is about. Sublime keeps it. Before a
/// field is shown: its state, when the right button goes down this frame.
pub fn state_before_right_press(ctx: &egui::Context, id: egui::Id) -> Option<TextEditState> {
    let pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
    pressed.then(|| TextEditState::load(ctx, id)).flatten()
}

/// After the field is shown: when the right button went down on it, its selection comes back (also
/// in the stored state); the selection to go on with.
pub fn keep_selection(
    ctx: &egui::Context,
    id: egui::Id,
    response: &egui::Response,
    kept: Option<TextEditState>,
) -> Option<CCursorRange> {
    let state = kept.filter(|_| response.hovered())?;
    let range = state.cursor.char_range();
    state.store(ctx, id);
    range
}

/// After the views: no context menu is open when none said so this frame.
pub fn report_closed(events: &mut Vec<UiEvent>) {
    if !events
        .iter()
        .any(|e| matches!(e, UiEvent::ContextMenu(Some(_))))
    {
        events.push(UiEvent::ContextMenu(None));
    }
}

/// A plugin's menu (`view.show_popup_menu`), where the mouse was when it appeared (the centre of
/// the window if the mouse is elsewhere). A click on an item chooses it; Esc or a click outside
/// dismisses it.
pub fn show_popup_menu(ctx: &egui::Context, menu: &PopupMenuViewModel, events: &mut Vec<UiEvent>) {
    let at = ctx
        .pointer_latest_pos()
        .unwrap_or_else(|| ctx.screen_rect().center());
    let response = egui::Area::new(egui::Id::new(("plugin_popup_menu", menu.serial)))
        .order(egui::Order::Foreground)
        .default_pos(at)
        .movable(false)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style())
                .show(ui, |ui| {
                    ui.set_min_width(160.0);
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        let mut chosen = None;
                        for (index, item) in menu.items.iter().enumerate() {
                            let button = egui::Button::new(item).frame(false);
                            if ui.add(button).clicked() {
                                chosen = Some(index);
                            }
                        }
                        chosen
                    })
                    .inner
                })
                .inner
        });
    if let Some(index) = response.inner {
        events.push(UiEvent::PopupMenuChosen(Some(index)));
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape))
        || response.response.clicked_elsewhere()
    {
        events.push(UiEvent::PopupMenuChosen(None));
    }
}
