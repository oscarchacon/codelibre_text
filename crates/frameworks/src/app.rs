//! The eframe application: each frame it feeds input to the controllers, asks the presenters for
//! view models, draws the views and hands the resulting UI events back to the controllers.
//!
//! Every window of the editor (`use_cases::windows`) is drawn in the same frame: the first in
//! the root viewport, the others in immediate viewports of their own. Each has its own UI state
//! ([`WindowUi`]); the editor switches to a window while it is drawn, and between frames the
//! window with the focus is the current one (`sublime.active_window()`).

use std::collections::HashMap;
use std::time::Duration;

use egui::{Event, Key, Modifiers, RichText};
use interface_adapters::controllers::keyboard_controller::{self, KeyHandling};
use interface_adapters::controllers::{command_controller, ui_controller};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::presenters;
use interface_adapters::view_models::GroupViewModel;
use interface_adapters::view_state::{ViewRequest, ViewState, WidgetEdit};
use use_cases::windows::WindowId;
use use_cases::Editor;

use crate::input;
use crate::syntax_engine::SharedSyntaxEngine;
use crate::views::editor_view::{self, EditorText, SelectionSync};
use crate::views::theme::Chrome;
#[cfg(not(target_os = "macos"))]
use crate::views::menu_bar;
#[cfg(target_os = "macos")]
use crate::views::native_menu::{Activation, NativeMenu};
use crate::views::{dialogs, find_panel, input_panel, output_panel, quick_panel, side_bar, status_bar, tabs};

/// What one window shows on screen beyond the editor's state: its panels, side bar, the carets
/// its text widgets applied, the widget edits waiting for its next frame.
#[derive(Default)]
struct WindowUi {
    view: ViewState,
    selection_sync: SelectionSync,
    /// Events for the text widget, delivered at the start of the next frame (undo, paste…).
    pending_widget_events: Vec<Event>,
    injected_event_count: usize,
    /// Keys the macOS menu bar took as key equivalents, given back to the keymap next frame.
    replayed_keys: Vec<Event>,
    title: String,
    /// Its first frame asks the system for the focus.
    shown: bool,
}

pub struct App {
    editor: Editor,
    engine: SharedSyntaxEngine,
    /// The UI of each window, by the editor's window id.
    windows: HashMap<WindowId, WindowUi>,
    /// The window the root viewport shows (another one when it was closed with others open).
    root: WindowId,
    /// The window with the system's focus: the current one between frames.
    focused: WindowId,
    /// macOS: the menu bar of the system, instead of one drawn in the window.
    #[cfg(target_os = "macos")]
    native_menu: NativeMenu,
    /// When plugins were last asked which of their menu commands are enabled, visible or checked.
    #[cfg(target_os = "macos")]
    command_states_asked: std::time::Instant,
    visuals_for_scheme: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, editor: Editor, view: ViewState, engine: SharedSyntaxEngine) -> Self {
        // Ctrl+=/- are the editor's font size commands, not egui's zoom.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        #[cfg(target_os = "macos")]
        add_key_symbols_font(&cc.egui_ctx);
        let root = editor.window_ids()[0];
        let windows = HashMap::from([(root, WindowUi { view, shown: true, ..WindowUi::default() })]);
        Self {
            editor,
            engine,
            windows,
            root,
            focused: root,
            #[cfg(target_os = "macos")]
            native_menu: NativeMenu::new(&cc.egui_ctx),
            #[cfg(target_os = "macos")]
            command_states_asked: std::time::Instant::now(),
            visuals_for_scheme: String::new(),
        }
    }

    /// Key presses go through the keymap; bound keys are removed so widgets do not see them.
    fn dispatch_keys(&mut self, ctx: &egui::Context, window: &mut WindowUi) {
        let view = &mut window.view;
        let events = ctx.input(|i| i.events.clone());
        let real_events = events.len().saturating_sub(window.injected_event_count);
        let mut consumed = Vec::new();
        let mut last_combos = Vec::new();
        for (index, event) in events.iter().enumerate().take(real_events) {
            match event {
                Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    let combos = input::key_combos(*key, *physical_key, *modifiers);
                    if let KeyHandling::Consumed = keyboard_controller::key_pressed(&mut self.editor, view, &combos) {
                        consumed.push(index);
                    }
                    last_combos = combos;
                }
                // A consumed key must not also type its character (e.g. Alt+1 typing "1").
                Event::Text(_) if index > 0 && consumed.last() == Some(&(index - 1)) => consumed.push(index),
                // Keymaps bind symbols by the character (`*`, `"`): asked once it is typed, unless
                // its key event already had that name.
                Event::Text(text) => {
                    let typed = input::typed_combo(text).filter(|combo| !last_combos.contains(combo));
                    if let Some(combo) = typed {
                        if let KeyHandling::Consumed = keyboard_controller::key_pressed(&mut self.editor, view, &[combo]) {
                            consumed.push(index);
                        }
                    }
                }
                // egui turns Ctrl+C/X/V into these events, without a key event.
                Event::Copy | Event::Cut | Event::Paste(_) => {
                    let command = match event {
                        Event::Copy => "copy",
                        Event::Cut => "cut",
                        _ => "paste",
                    };
                    if let KeyHandling::Consumed = keyboard_controller::clipboard_shortcut(&mut self.editor, view, command) {
                        consumed.push(index);
                    }
                }
                _ => {}
            }
        }
        if !consumed.is_empty() {
            ctx.input_mut(|i| {
                let mut index = 0;
                i.events.retain(|_| {
                    let keep = !consumed.contains(&index);
                    index += 1;
                    keep
                });
            });
        }
    }

    fn run_queued_commands(&mut self, view: &mut ViewState) {
        // Commands can queue further commands (e.g. a palette entry); run until none are left.
        for _ in 0..16 {
            let commands = view.take_commands();
            if commands.is_empty() {
                break;
            }
            for command in commands {
                command_controller::run(&mut self.editor, view, &command);
            }
        }
    }

    fn carry_out_view_requests(&mut self, ctx: &egui::Context, id: WindowId, window: &mut WindowUi) {
        let ctrl = Modifiers { ctrl: true, command: true, ..Default::default() };
        let key = |key: Key, modifiers: Modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for request in window.view.take_requests() {
            match request {
                ViewRequest::SetFullscreen(on) => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on)),
                // The last window closes the application; another one just goes.
                ViewRequest::CloseWindow if self.editor.window_ids().len() > 1 => {
                    self.editor.close_window(id);
                }
                ViewRequest::CloseWindow | ViewRequest::Exit => ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close),
                ViewRequest::WidgetEdit(edit) => {
                    window.pending_widget_events.push(match edit {
                        WidgetEdit::Undo => key(Key::Z, ctrl),
                        WidgetEdit::Redo => key(Key::Z, Modifiers { shift: true, ..ctrl }),
                        WidgetEdit::SelectAll => key(Key::A, ctrl),
                        WidgetEdit::Copy => Event::Copy,
                        WidgetEdit::Cut => Event::Cut,
                        WidgetEdit::Paste(text) => Event::Paste(text),
                    });
                    window.view.focus_editor = true;
                    // The widget gets it in the next frame, which must come even with no input.
                    ctx.request_repaint();
                }
            }
        }
    }

    /// macOS: runs what was chosen in the menu bar. Its keys go back to the keymap (next frame, as
    /// if the window had got them), as egui would have seen them: Cmd+C/X/V as copy / cut / paste.
    #[cfg(target_os = "macos")]
    fn handle_native_menu(&mut self, ctx: &egui::Context, window: &mut WindowUi) {
        for activation in self.native_menu.activations() {
            match activation {
                Activation::Command(command) => ui_controller::handle(&mut self.editor, &mut window.view, UiEvent::RunCommand(command)),
                Activation::Key(combo) => {
                    let Some((key, modifiers)) = input::key_event(&combo) else { continue };
                    let clipboard = modifiers.command && !modifiers.shift && !modifiers.alt && !modifiers.ctrl;
                    window.replayed_keys.push(match key {
                        Key::C if clipboard => Event::Copy,
                        Key::X if clipboard => Event::Cut,
                        Key::V if clipboard => match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                            Ok(text) if !text.is_empty() => Event::Paste(text.replace("\r\n", "\n")),
                            _ => continue,
                        },
                        _ => Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
                    });
                    ctx.request_repaint();
                }
            }
        }
        // A native menu cannot ask when it opens, as the egui one does (`UiEvent::MenuOpen`).
        if self.command_states_asked.elapsed() >= Duration::from_secs(1) && ctx.input(|i| i.focused) {
            self.editor.refresh_plugin_command_states();
            self.command_states_asked = std::time::Instant::now();
        }
    }

    /// Closing a window. The root viewport with other windows open: only its window goes, and the
    /// viewport shows another one. The last window: hot exit saves the session (every window);
    /// otherwise unsaved changes are asked about. Another window: its files close (asking).
    /// Returns false when the window is gone.
    fn handle_close_request(&mut self, ctx: &egui::Context, id: WindowId, window: &mut WindowUi) -> bool {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return true;
        }
        if self.editor.window_ids().len() > 1 && !window.view.exit_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return !self.editor.close_window(id);
        }
        let layout = window.view.layout();
        if window.view.exit_confirmed {
            self.editor.save_session(layout);
        } else if !self.editor.request_exit(layout) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return true;
        }
        self.editor.plugin_exit();
        true
    }

    /// One window's frame: its keys, commands, drawing and events. False when it closed.
    fn window_frame(&mut self, ctx: &egui::Context, id: WindowId, root: bool) -> bool {
        if !self.editor.switch_to_window(id) {
            return false;
        }
        let mut window = self.windows.remove(&id).unwrap_or_default();
        if !root {
            // The root viewport's are put in its raw input (`raw_input_hook`); others' here.
            window.injected_event_count = window.pending_widget_events.len();
            ctx.input_mut(|i| i.events.append(&mut window.pending_widget_events));
        }
        if !window.shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            window.shown = true;
        }
        if !self.handle_close_request(ctx, id, &mut window) {
            return false;
        }

        let text_area = self.editor.active_document().map(|d| editor_view::widget_id(d.id));
        let focused = ctx.memory(|m| m.focused());
        window.view.text_has_focus = text_area.is_some() && focused == text_area;
        // The focus went to another group (a key, a plugin): its text takes the keyboard from the
        // text of the group that had it.
        if focused.is_some() && !window.view.text_has_focus {
            let shown: Vec<usize> = (0..self.editor.group_count()).filter_map(|g| self.editor.active_index_in_group(g)).collect();
            let documents = &self.editor.state().documents;
            if shown.into_iter().any(|i| Some(editor_view::widget_id(documents[i].id)) == focused) {
                window.view.focus_editor = true;
            }
        }
        self.dispatch_keys(ctx, &mut window);
        #[cfg(target_os = "macos")]
        if root {
            self.handle_native_menu(ctx, &mut window);
        }
        self.run_queued_commands(&mut window.view);
        self.carry_out_view_requests(ctx, id, &mut window);

        let scheme = self.editor.color_scheme_name();
        let chrome = Chrome::from_scheme(self.engine.borrow().scheme(&scheme));
        if root && self.visuals_for_scheme != scheme {
            ctx.set_visuals(chrome.visuals());
            self.visuals_for_scheme = scheme;
        } else if !root {
            ctx.set_visuals(chrome.visuals());
        }

        let dialogs = id == self.focused || !self.windows.contains_key(&self.focused);
        let events = self.draw(ctx, &chrome, &mut window, root, dialogs);
        // This frame shows the state before its events: draw what they changed (a closed panel, a
        // new language) now, not at the next input, or the panel seems to need another click.
        if !events.is_empty() {
            ctx.request_repaint();
        }
        for event in events {
            ui_controller::handle(&mut self.editor, &mut window.view, event);
        }
        // After the text widget's edits, so listeners hear about them this frame.
        command_controller::poll_plugins(&mut self.editor, &mut window.view);
        self.editor.finish_closing_windows();

        let title = presenters::window::present_title(&self.editor);
        if title != window.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            window.title = title;
        }
        if ctx.input(|i| i.focused) {
            self.focused = id;
        }
        let still_open = self.editor.window_ids().contains(&id);
        if still_open {
            self.windows.insert(id, window);
        }
        still_open
    }

    fn draw(&mut self, ctx: &egui::Context, chrome: &Chrome, window: &mut WindowUi, root: bool, dialogs: bool) -> Vec<UiEvent> {
        let mut events = Vec::new();
        let view = &mut window.view;
        // Views look up their fixed labels in the interface language.
        let translations = self.editor.translations();

        #[cfg(target_os = "macos")]
        if root {
            self.native_menu.show(presenters::menu::present_for_macos(&self.editor));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = root;
            let menu = presenters::menu::present(&self.editor);
            egui::TopBottomPanel::top("menu_bar")
                .frame(egui::Frame::new().fill(chrome.panel_bg).inner_margin(egui::Margin::symmetric(6, 3)))
                .show(ctx, |ui| menu_bar::show(ui, &menu, &mut events));
        }

        if view.show_status_bar {
            let status = presenters::status_bar::present(&self.editor);
            egui::TopBottomPanel::bottom("status_bar")
                .exact_height(24.0)
                .frame(egui::Frame::new().fill(chrome.panel_bg))
                .show(ctx, |ui| status_bar::show(ui, chrome, &translations, &status, &mut events));
        }

        if let Some(input) = presenters::quick_panel::present_input_panel(&self.editor, view) {
            egui::TopBottomPanel::bottom("input_panel")
                .frame(
                    egui::Frame::new()
                        .fill(chrome.panel_bg)
                        .inner_margin(egui::Margin::symmetric(8, 0))
                        .stroke(egui::Stroke::new(1.0_f32, chrome.border)),
                )
                .show(ctx, |ui| input_panel::show(ui, chrome, &input, &mut events));
        }

        if let Some(output) = presenters::output_panel::present(&self.editor) {
            egui::TopBottomPanel::bottom("output_panel")
                .resizable(true)
                .default_height(180.0)
                .height_range(60.0..=ctx.screen_rect().height() * 0.7)
                .frame(
                    egui::Frame::new()
                        .fill(chrome.editor_bg)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .stroke(egui::Stroke::new(1.0_f32, chrome.border)),
                )
                .show(ctx, |ui| output_panel::show(ui, chrome, &output, &mut events));
        }

        if let Some(find) = presenters::find_panel::present(&mut self.editor, view) {
            egui::TopBottomPanel::bottom("find_panel")
                .frame(
                    egui::Frame::new()
                        .fill(chrome.panel_bg)
                        .inner_margin(egui::Margin::symmetric(8, 0))
                        .stroke(egui::Stroke::new(1.0_f32, chrome.border)),
                )
                .show(ctx, |ui| find_panel::show(ui, chrome, &translations, &find, &mut events));
        }

        if view.show_side_bar {
            let side = presenters::side_bar::present(&self.editor, view);
            egui::SidePanel::left("side_bar")
                .default_width(230.0)
                .width_range(120.0..=600.0)
                .frame(egui::Frame::new().fill(chrome.panel_bg))
                .show(ctx, |ui| side_bar::show(ui, chrome, &translations, &side, &mut events));
        }

        let groups = presenters::groups::present(&mut self.editor, view);
        let hints = presenters::editor::present_hints(&self.editor);
        let selection_sync = &mut window.selection_sync;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(chrome.editor_bg))
            .show(ctx, |ui| {
                let area = ui.available_rect_before_wrap();
                let at = |x: f32, y: f32| egui::pos2(area.left() + x * area.width(), area.top() + y * area.height());
                for group in &groups {
                    let [left, top, right, bottom] = group.rect;
                    let rect = egui::Rect::from_min_max(at(left, top), at(right, bottom));
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("group", group.group)));
                    child.set_clip_rect(rect.intersect(ui.clip_rect()));
                    show_group(&mut child, &mut self.editor, &self.engine, selection_sync, chrome, group, &hints, &mut events);
                    // A tab dropped on a group's text goes to the end of its tabs.
                    drop_tab_on_group(ui, rect, group.group, chrome, &mut events);
                    // A line between groups.
                    let stroke = egui::Stroke::new(1.0_f32, chrome.border);
                    if right < 1.0 {
                        ui.painter().line_segment([rect.right_top(), rect.right_bottom()], stroke);
                    }
                    if bottom < 1.0 {
                        ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
                    }
                }
                drag_layout_boundaries(ui, area, self.editor.layout(), &mut events);
            });

        let view = &mut window.view;
        if let Some(panel) = presenters::quick_panel::present(&self.editor, view) {
            quick_panel::show(ctx, chrome, &panel, &mut events);
        }
        // The question about unsaved files shows in one window: the one with the focus.
        if dialogs {
            if let Some(dialog) = presenters::dialogs::present_close_dialog(&self.editor) {
                dialogs::show_close_dialog(ctx, &translations, &dialog, &mut events);
            }
        }
        if let Some(about) = presenters::dialogs::present_about(&self.editor, view) {
            dialogs::show_about(ctx, &translations, &about, &mut events);
        }
        if let Some(errors) = presenters::dialogs::present_load_errors(&self.editor, view) {
            dialogs::show_load_errors(ctx, &translations, &errors, &mut events);
        }
        events
    }

    /// Windows the editor opened get their UI (and a viewport); those it closed lose it.
    fn follow_windows(&mut self) {
        let ids = self.editor.window_ids();
        self.windows.retain(|id, _| ids.contains(id));
        if !ids.contains(&self.root) {
            self.root = ids[0];
        }
        for id in ids {
            self.windows.entry(id).or_default();
        }
        if !self.windows.contains_key(&self.focused) {
            self.focused = self.root;
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let Some(window) = self.windows.get_mut(&self.root) else { return };
        raw_input.events.append(&mut window.replayed_keys);
        window.injected_event_count = window.pending_widget_events.len();
        raw_input.events.append(&mut window.pending_widget_events);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.editor.poll_file_index() {
            ctx.request_repaint();
        }
        self.editor.poll_builds();
        self.editor.poll_downloads();
        self.editor.check_external_changes();
        self.follow_windows();

        let root = self.root;
        self.window_frame(ctx, root, true);
        self.follow_windows();
        let others: Vec<WindowId> = self.editor.window_ids().into_iter().filter(|id| *id != self.root).collect();
        for id in others {
            let title = self.windows.get(&id).map(|w| w.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| "CodeLibre Text".into());
            let builder = egui::ViewportBuilder::default().with_title(title).with_inner_size([1000.0, 700.0]);
            ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("window", id)), builder, |ctx, _class| {
                self.window_frame(ctx, id, false);
            });
        }
        self.follow_windows();
        // Between frames (timers, plugin threads) the window with the focus is the current one.
        self.editor.switch_to_window(self.focused);

        // An auto-complete list waiting for `auto_complete_delay` opens on time.
        if let Some(due) = self.editor.auto_complete_due() {
            ctx.request_repaint_after(due.saturating_duration_since(std::time::Instant::now()));
        }
        // Keep status messages, the file index and external-change checks moving.
        if self.editor.is_indexing_files() || self.editor.state().status.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }
}

/// The lines between the layout's columns and rows can be dragged: the boundary follows the
/// mouse (kept away from its neighbours).
fn drag_layout_boundaries(ui: &mut egui::Ui, area: egui::Rect, layout: &entities::layout::Layout, events: &mut Vec<UiEvent>) {
    let pointer = ui.ctx().pointer_interact_pos();
    let handles = layout.cols.iter().enumerate().map(|(i, x)| (true, i, *x)).chain(layout.rows.iter().enumerate().map(|(i, y)| (false, i, *y)));
    for (vertical, index, at) in handles {
        let count = if vertical { layout.cols.len() } else { layout.rows.len() };
        if index == 0 || index + 1 >= count {
            continue;
        }
        let rect = if vertical {
            let x = area.left() + at as f32 * area.width();
            egui::Rect::from_min_max(egui::pos2(x - 3.0, area.top()), egui::pos2(x + 3.0, area.bottom()))
        } else {
            let y = area.top() + at as f32 * area.height();
            egui::Rect::from_min_max(egui::pos2(area.left(), y - 3.0), egui::pos2(area.right(), y + 3.0))
        };
        let response = ui.interact(rect, ui.id().with(("layout_boundary", vertical, index)), egui::Sense::DRAG);
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
        }
        if let (true, Some(pointer)) = (response.dragged(), pointer) {
            let fraction = if vertical { (pointer.x - area.left()) / area.width() } else { (pointer.y - area.top()) / area.height() };
            events.push(UiEvent::LayoutBoundaryDragged { vertical, index, fraction: fraction as f64 });
        }
    }
}

/// While a tab is dragged over a group, the group is marked; let go there, the tab moves to the
/// end of the group's tabs (dropped on its tab strip, the tab bar placed it already).
fn drop_tab_on_group(ui: &egui::Ui, rect: egui::Rect, group: usize, chrome: &Chrome, events: &mut Vec<UiEvent>) {
    let ctx = ui.ctx();
    let Some(dragged) = egui::DragAndDrop::payload::<tabs::DraggedTab>(ctx) else { return };
    let Some(pointer) = ctx.pointer_hover_pos().filter(|p| rect.contains(*p)) else { return };
    let over_tabs = pointer.y < rect.top() + 34.0;
    if over_tabs {
        return;
    }
    ui.painter().rect_stroke(rect.shrink(2.0), 2.0, egui::Stroke::new(2.0_f32, chrome.accent), egui::StrokeKind::Inside);
    if ctx.input(|i| i.pointer.any_released()) {
        egui::DragAndDrop::clear_payload(ctx);
        events.push(UiEvent::TabDropped { tab: dragged.0, group, index: usize::MAX });
    }
}

/// One group of the layout: its tab bar and its document (or, when it has none, the hints).
#[allow(clippy::too_many_arguments)]
fn show_group(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    engine: &SharedSyntaxEngine,
    selection_sync: &mut SelectionSync,
    chrome: &Chrome,
    group: &GroupViewModel,
    hints: &[String],
    events: &mut Vec<UiEvent>,
) {
    egui::Frame::new()
        .fill(chrome.panel_bg)
        .inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 0 })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            tabs::show(ui, chrome, group.group, group.active, &group.tabs, events);
        });

    let Some(model) = &group.editor else {
        let rest = ui.available_rect_before_wrap();
        if ui.interact(rest, ui.id().with("empty_group"), egui::Sense::CLICK).clicked() {
            events.push(UiEvent::FocusGroup(group.group));
        }
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(hints.join("\n")).color(chrome.fg_dim).size(14.0));
        });
        return;
    };
    let read_only_copy;
    let text = if editor.text_for_editing(model.document).is_some() {
        EditorText::Editable(editor.text_for_editing(model.document).expect("checked above"))
    } else {
        read_only_copy = editor.state().documents.iter().find(|d| d.id == model.document).map(|d| d.text.clone()).unwrap_or_default();
        EditorText::ReadOnly(&read_only_copy)
    };
    egui::Frame::new().fill(chrome.editor_bg).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        editor_view::show(ui, model, text, engine, selection_sync, events);
    });
}

/// macOS writes shortcuts with symbols (⌃⌥⇧⌘⎋⌫…) that egui's fonts partly lack or draw thin: the
/// system's San Francisco font draws them as its menus do, right after egui's main font, and Apple
/// Symbols fills in the rest (⇞ ⇟).
#[cfg(target_os = "macos")]
fn add_key_symbols_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, path, position) in [
        ("san_francisco", "/System/Library/Fonts/SFNS.ttf", 1),
        ("apple_symbols", "/System/Library/Fonts/Apple Symbols.ttf", usize::MAX),
    ] {
        let Ok(bytes) = std::fs::read(path) else {
            tracing::warn!("font {path} not found: some shortcut symbols may not show");
            continue;
        };
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.entry(family).or_default();
            list.insert(position.min(list.len()), name.into());
        }
    }
    ctx.set_fonts(fonts);
}
