//! The menu in the macOS menu bar, built with `muda` from the same entries the other systems draw
//! inside the window ([`menu_bar`](super::menu_bar)). It is rebuilt only when they change.
//!
//! A shortcut shown in a native menu is a key equivalent: AppKit runs the item and the window
//! never gets the key. Such keys are given back ([`Activation::Key`]) so the keymap decides, with
//! its contexts, as for any other key.

use std::sync::mpsc::{self, Receiver};

use entities::command::Command;
use entities::keymap::KeyCombo;
use interface_adapters::view_models::{MacMenuViewModel, MenuEntryViewModel};
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEventType};

/// What the user chose in the menu bar.
pub enum Activation {
    /// A click on an item.
    Command(Command),
    /// The key of an item, pressed in the window.
    Key(KeyCombo),
}

pub struct NativeMenu {
    shown: Option<MacMenuViewModel>,
    /// The menu bar must outlive its use by AppKit.
    menu: Option<Menu>,
    /// What each item runs, by its id (the index).
    items: Vec<(Command, Option<KeyCombo>)>,
    activations: Receiver<(MenuId, bool)>,
}

impl NativeMenu {
    pub fn new(ctx: &egui::Context) -> Self {
        let (sender, activations) = mpsc::channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = sender.send((event.id, pressed_a_key()));
            ctx.request_repaint();
        }));
        Self { shown: None, menu: None, items: Vec::new(), activations }
    }

    /// Puts `model` in the menu bar, unless it is already there.
    pub fn show(&mut self, model: MacMenuViewModel) {
        if self.shown.as_ref() == Some(&model) {
            return;
        }
        self.items.clear();
        match self.build(&model) {
            Ok((menu, help)) => {
                menu.init_for_nsapp();
                // macOS adds its search field to the Help menu.
                if let Some(help) = help {
                    help.set_as_help_menu_for_nsapp();
                }
                self.menu = Some(menu);
            }
            Err(e) => tracing::warn!("could not build the macOS menu bar: {e}"),
        }
        self.shown = Some(model);
    }

    /// The items chosen since the last call.
    pub fn activations(&self) -> Vec<Activation> {
        self.activations
            .try_iter()
            .filter_map(|(id, pressed_a_key)| {
                let (command, key) = self.items.get(id.as_ref().parse::<usize>().ok()?)?;
                Some(match key {
                    Some(key) if pressed_a_key => Activation::Key(key.clone()),
                    _ => Activation::Command(command.clone()),
                })
            })
            .collect()
    }

    /// The menu bar, and its Help menu.
    fn build(&mut self, model: &MacMenuViewModel) -> muda::Result<(Menu, Option<Submenu>)> {
        let menu = Menu::new();
        let app = Submenu::new(&model.app_name, true);
        if let Some(about) = &model.about {
            self.append(&app, about)?;
            app.append(&PredefinedMenuItem::separator())?;
        }
        if let Some(preferences) = &model.preferences {
            self.append(&app, preferences)?;
            app.append(&PredefinedMenuItem::separator())?;
        }
        app.append(&PredefinedMenuItem::services(Some(&model.services)))?;
        app.append(&PredefinedMenuItem::separator())?;
        app.append(&PredefinedMenuItem::hide(Some(&model.hide)))?;
        app.append(&PredefinedMenuItem::hide_others(Some(&model.hide_others)))?;
        app.append(&PredefinedMenuItem::show_all(Some(&model.show_all)))?;
        if let Some((quit, command)) = model.quit.as_ref().and_then(|quit| Some((quit, quit.command.as_ref()?))) {
            app.append(&PredefinedMenuItem::separator())?;
            // Cmd+Q, even when the keymap binds no key to `exit`.
            let accelerator = quit.key.as_ref().and_then(accelerator).or_else(|| "cmd+q".parse().ok());
            let id = self.add_item(command, &quit.key);
            app.append(&MenuItem::with_id(id, &quit.caption, quit.enabled, accelerator))?;
        }
        menu.append(&app)?;
        let mut help = None;
        for top in &model.menus {
            let submenu = Submenu::new(&top.caption, true);
            for entry in &top.children {
                self.append(&submenu, entry)?;
            }
            menu.append(&submenu)?;
            if top.id.as_deref() == Some("help") {
                help = Some(submenu);
            }
        }
        Ok((menu, help))
    }

    fn append(&mut self, parent: &Submenu, entry: &MenuEntryViewModel) -> muda::Result<()> {
        if entry.separator {
            return parent.append(&PredefinedMenuItem::separator());
        }
        if !entry.children.is_empty() {
            let submenu = Submenu::new(&entry.caption, true);
            for child in &entry.children {
                self.append(&submenu, child)?;
            }
            return parent.append(&submenu);
        }
        let Some(command) = &entry.command else {
            return parent.append(&MenuItem::new(&entry.caption, false, None));
        };
        let accelerator = entry.key.as_ref().and_then(accelerator);
        let id = self.add_item(command, &entry.key);
        match entry.checked {
            Some(checked) => parent.append(&CheckMenuItem::with_id(id, &entry.caption, entry.enabled, checked, accelerator)),
            None => parent.append(&MenuItem::with_id(id, &entry.caption, entry.enabled, accelerator)),
        }
    }

    /// Remembers what an item runs; returns its id.
    fn add_item(&mut self, command: &Command, key: &Option<KeyCombo>) -> String {
        self.items.push((command.clone(), key.clone()));
        (self.items.len() - 1).to_string()
    }
}

/// A key equivalent for `key`. Keys without Cmd or Ctrl (other than F1–F20) are left out: as key
/// equivalents they would no longer type or move the caret.
fn accelerator(key: &KeyCombo) -> Option<Accelerator> {
    let function_key = key.key.len() > 1 && key.key.starts_with('f') && key.key[1..].chars().all(|c| c.is_ascii_digit());
    if !(key.ctrl || key.super_key || function_key) {
        return None;
    }
    let mut spec = String::new();
    for (on, modifier) in [(key.ctrl, "ctrl+"), (key.alt, "alt+"), (key.shift, "shift+"), (key.super_key, "cmd+")] {
        if on {
            spec.push_str(modifier);
        }
    }
    spec.push_str(&key.key);
    spec.parse().ok()
}

/// Whether the item being run was chosen with its key (not clicked).
fn pressed_a_key() -> bool {
    MainThreadMarker::new()
        .and_then(|mtm| NSApplication::sharedApplication(mtm).currentEvent())
        .is_some_and(|event| event.r#type() == NSEventType::KeyDown)
}
