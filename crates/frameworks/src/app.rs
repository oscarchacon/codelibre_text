//! The eframe application: each frame it feeds input to the controllers, asks the presenters for
//! view models, draws the views and hands the resulting UI events back to the controllers.

use std::time::Duration;

use egui::{Event, Key, Modifiers, RichText};
use interface_adapters::controllers::keyboard_controller::{self, KeyHandling};
use interface_adapters::controllers::{command_controller, ui_controller};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::presenters;
use interface_adapters::view_state::{ViewRequest, ViewState, WidgetEdit};
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

pub struct App {
    editor: Editor,
    view: ViewState,
    engine: SharedSyntaxEngine,
    selection_sync: SelectionSync,
    /// Events for the text widget, delivered at the start of the next frame (undo, paste…).
    pending_widget_events: Vec<Event>,
    injected_event_count: usize,
    /// Keys the macOS menu bar took as key equivalents, given back to the keymap next frame.
    replayed_keys: Vec<Event>,
    /// macOS: the menu bar of the system, instead of one drawn in the window.
    #[cfg(target_os = "macos")]
    native_menu: NativeMenu,
    /// When plugins were last asked which of their menu commands are enabled, visible or checked.
    #[cfg(target_os = "macos")]
    command_states_asked: std::time::Instant,
    window_title: String,
    visuals_for_scheme: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, editor: Editor, view: ViewState, engine: SharedSyntaxEngine) -> Self {
        // Ctrl+=/- are the editor's font size commands, not egui's zoom.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        #[cfg(target_os = "macos")]
        add_key_symbols_font(&cc.egui_ctx);
        Self {
            editor,
            view,
            engine,
            selection_sync: SelectionSync::default(),
            pending_widget_events: Vec::new(),
            injected_event_count: 0,
            replayed_keys: Vec::new(),
            #[cfg(target_os = "macos")]
            native_menu: NativeMenu::new(&cc.egui_ctx),
            #[cfg(target_os = "macos")]
            command_states_asked: std::time::Instant::now(),
            window_title: String::new(),
            visuals_for_scheme: String::new(),
        }
    }

    /// Key presses go through the keymap; bound keys are removed so widgets do not see them.
    fn dispatch_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let real_events = events.len().saturating_sub(self.injected_event_count);
        let mut consumed = Vec::new();
        let mut last_combos = Vec::new();
        for (index, event) in events.iter().enumerate().take(real_events) {
            match event {
                Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    let combos = input::key_combos(*key, *physical_key, *modifiers);
                    if let KeyHandling::Consumed = keyboard_controller::key_pressed(&mut self.editor, &mut self.view, &combos) {
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
                        if let KeyHandling::Consumed = keyboard_controller::key_pressed(&mut self.editor, &mut self.view, &[combo]) {
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
                    if let KeyHandling::Consumed = keyboard_controller::clipboard_shortcut(&mut self.editor, &mut self.view, command) {
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

    fn run_queued_commands(&mut self) {
        // Commands can queue further commands (e.g. a palette entry); run until none are left.
        for _ in 0..16 {
            let commands = self.view.take_commands();
            if commands.is_empty() {
                break;
            }
            for command in commands {
                command_controller::run(&mut self.editor, &mut self.view, &command);
            }
        }
    }

    fn carry_out_view_requests(&mut self, ctx: &egui::Context) {
        let ctrl = Modifiers { ctrl: true, command: true, ..Default::default() };
        let key = |key: Key, modifiers: Modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for request in self.view.take_requests() {
            match request {
                ViewRequest::SetFullscreen(on) => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on)),
                ViewRequest::CloseWindow => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                ViewRequest::WidgetEdit(edit) => {
                    self.pending_widget_events.push(match edit {
                        WidgetEdit::Undo => key(Key::Z, ctrl),
                        WidgetEdit::Redo => key(Key::Z, Modifiers { shift: true, ..ctrl }),
                        WidgetEdit::SelectAll => key(Key::A, ctrl),
                        WidgetEdit::Copy => Event::Copy,
                        WidgetEdit::Cut => Event::Cut,
                        WidgetEdit::Paste(text) => Event::Paste(text),
                    });
                    self.view.focus_editor = true;
                    // The widget gets it in the next frame, which must come even with no input.
                    ctx.request_repaint();
                }
            }
        }
    }

    /// macOS: runs what was chosen in the menu bar. Its keys go back to the keymap (next frame, as
    /// if the window had got them), as egui would have seen them: Cmd+C/X/V as copy / cut / paste.
    #[cfg(target_os = "macos")]
    fn handle_native_menu(&mut self, ctx: &egui::Context) {
        for activation in self.native_menu.activations() {
            match activation {
                Activation::Command(command) => ui_controller::handle(&mut self.editor, &mut self.view, UiEvent::RunCommand(command)),
                Activation::Key(combo) => {
                    let Some((key, modifiers)) = input::key_event(&combo) else { continue };
                    let clipboard = modifiers.command && !modifiers.shift && !modifiers.alt && !modifiers.ctrl;
                    self.replayed_keys.push(match key {
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

    /// Closing the window: hot exit saves the session; otherwise unsaved changes are asked about.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let layout = self.view.layout();
        if self.view.exit_confirmed {
            self.editor.save_session(layout);
        } else if !self.editor.request_exit(layout) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        self.editor.plugin_exit();
    }

    fn draw(&mut self, ctx: &egui::Context, chrome: &Chrome) -> Vec<UiEvent> {
        let mut events = Vec::new();
        // Views look up their fixed labels in the interface language.
        let translations = self.editor.translations();

        #[cfg(target_os = "macos")]
        self.native_menu.show(presenters::menu::present_for_macos(&self.editor));
        #[cfg(not(target_os = "macos"))]
        {
            let menu = presenters::menu::present(&self.editor);
            egui::TopBottomPanel::top("menu_bar")
                .frame(egui::Frame::new().fill(chrome.panel_bg).inner_margin(egui::Margin::symmetric(6, 3)))
                .show(ctx, |ui| menu_bar::show(ui, &menu, &mut events));
        }

        if self.view.show_status_bar {
            let status = presenters::status_bar::present(&self.editor);
            egui::TopBottomPanel::bottom("status_bar")
                .exact_height(24.0)
                .frame(egui::Frame::new().fill(chrome.panel_bg))
                .show(ctx, |ui| status_bar::show(ui, chrome, &translations, &status, &mut events));
        }

        if let Some(input) = presenters::quick_panel::present_input_panel(&self.editor, &mut self.view) {
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

        if let Some(find) = presenters::find_panel::present(&mut self.editor, &mut self.view) {
            egui::TopBottomPanel::bottom("find_panel")
                .frame(
                    egui::Frame::new()
                        .fill(chrome.panel_bg)
                        .inner_margin(egui::Margin::symmetric(8, 0))
                        .stroke(egui::Stroke::new(1.0_f32, chrome.border)),
                )
                .show(ctx, |ui| find_panel::show(ui, chrome, &translations, &find, &mut events));
        }

        if self.view.show_side_bar {
            let side = presenters::side_bar::present(&self.editor, &mut self.view);
            egui::SidePanel::left("side_bar")
                .default_width(230.0)
                .width_range(120.0..=600.0)
                .frame(egui::Frame::new().fill(chrome.panel_bg))
                .show(ctx, |ui| side_bar::show(ui, chrome, &translations, &side, &mut events));
        }

        let tab_list = presenters::tabs::present(&self.editor);
        let editor_model = presenters::editor::present(&mut self.editor, &mut self.view);
        let hints = presenters::editor::present_hints(&self.editor);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(chrome.editor_bg))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(chrome.panel_bg)
                    .inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 0 })
                    .show(ui, |ui| tabs::show(ui, chrome, &tab_list, &mut events));

                let Some(model) = editor_model else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(hints.join("\n")).color(chrome.fg_dim).size(14.0));
                    });
                    return;
                };
                let read_only_copy;
                let text = match self.editor.text_for_editing(model.document) {
                    Some(text) => EditorText::Editable(text),
                    None => {
                        read_only_copy = self.editor.active_document().map(|d| d.text.clone()).unwrap_or_default();
                        EditorText::ReadOnly(&read_only_copy)
                    }
                };
                egui::Frame::new().fill(chrome.editor_bg).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    editor_view::show(ui, &model, text, &self.engine, &mut self.selection_sync, &mut events);
                });
            });

        if let Some(panel) = presenters::quick_panel::present(&self.editor, &mut self.view) {
            quick_panel::show(ctx, chrome, &panel, &mut events);
        }
        if let Some(dialog) = presenters::dialogs::present_close_dialog(&self.editor) {
            dialogs::show_close_dialog(ctx, &translations, &dialog, &mut events);
        }
        if let Some(about) = presenters::dialogs::present_about(&self.editor, &self.view) {
            dialogs::show_about(ctx, &translations, &about, &mut events);
        }
        if let Some(errors) = presenters::dialogs::present_load_errors(&self.editor, &self.view) {
            dialogs::show_load_errors(ctx, &translations, &errors, &mut events);
        }
        events
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        raw_input.events.append(&mut self.replayed_keys);
        self.injected_event_count = self.pending_widget_events.len();
        raw_input.events.append(&mut self.pending_widget_events);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.editor.poll_file_index() {
            ctx.request_repaint();
        }
        self.editor.poll_builds();
        self.editor.poll_downloads();
        self.editor.check_external_changes();
        self.handle_close_request(ctx);

        let text_area = self.editor.active_document().map(|d| editor_view::widget_id(d.id));
        self.view.text_has_focus = text_area.is_some() && ctx.memory(|m| m.focused()) == text_area;
        self.dispatch_keys(ctx);
        #[cfg(target_os = "macos")]
        self.handle_native_menu(ctx);
        self.run_queued_commands();
        self.carry_out_view_requests(ctx);

        let scheme = self.editor.color_scheme_name();
        let chrome = Chrome::from_scheme(self.engine.borrow().scheme(&scheme));
        if self.visuals_for_scheme != scheme {
            ctx.set_visuals(chrome.visuals());
            self.visuals_for_scheme = scheme;
        }

        let events = self.draw(ctx, &chrome);
        // This frame shows the state before its events: draw what they changed (a closed panel, a
        // new language) now, not at the next input, or the panel seems to need another click.
        if !events.is_empty() {
            ctx.request_repaint();
        }
        for event in events {
            ui_controller::handle(&mut self.editor, &mut self.view, event);
        }
        // After the text widget's edits, so listeners hear about them this frame.
        command_controller::poll_plugins(&mut self.editor, &mut self.view);

        let title = presenters::window::present_title(&self.editor);
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        // Keep status messages, the file index and external-change checks moving.
        if self.editor.is_indexing_files() || self.editor.state().status.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }
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
