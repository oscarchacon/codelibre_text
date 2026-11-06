//! Use cases: the popups and phantoms plugins show (minihtml, see `entities::minihtml`), the links
//! clicked in them (`on_navigate`), and `on_hover`.
//!
//! One popup at a time, on one view (Sublime's too). It goes away when the view's caret moves
//! (unless `KEEP_ON_SELECTION_MODIFIED`), another view becomes active, the user presses Esc or
//! moves the mouse away from it (`HIDE_ON_MOUSE_MOVE*`, the view tells), or a plugin hides it;
//! its `on_hide` then runs at the next [`Editor::poll_plugins`], since the popup may go in the
//! middle of a plugin's call. Phantoms live in their [`Document`](entities::document::Document),
//! whose edits move them.

use std::rc::Rc;

use entities::document::{DocumentId, Phantom};
use entities::minihtml::data_uri_bytes;
use entities::selection::Selection;
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::plugins::CommandRunner;

impl Editor {
    /// The bytes of an image of a plugin's minihtml (`<img src>`): a `data:` URI, a resource
    /// (`res://Packages/…`) or a file (`file://…`). Read once; `None` when it cannot be.
    pub fn html_image(&self, src: &str) -> Option<Rc<Vec<u8>>> {
        if let Some(cached) = self.state.html_images.borrow().get(src) {
            return cached.clone();
        }
        let bytes = if let Some(bytes) = data_uri_bytes(src) {
            Some(bytes)
        } else if let Some(name) = src.strip_prefix("res://") {
            let include = self.global_settings().bool("import_sublime_packages", true);
            self.gateways.resources.load_resource(name.trim_start_matches('/'), include).ok()
        } else if let Some(path) = src.strip_prefix("file://") {
            // `file:///C:/x.png` and `file:///home/x.png`.
            let path = if path.len() > 3 && path.as_bytes()[2] == b':' { &path[1..] } else { path };
            self.gateways.file_system.read(std::path::Path::new(path)).ok()
        } else {
            None
        };
        let bytes = bytes.map(Rc::new);
        self.state.html_images.borrow_mut().insert(src.to_string(), bytes.clone());
        bytes
    }
}

/// Sublime's `HIDE_ON_MOUSE_MOVE`, `HIDE_ON_MOUSE_MOVE_AWAY`, `KEEP_ON_SELECTION_MODIFIED`.
pub const HIDE_ON_MOUSE_MOVE: u64 = 4;
pub const HIDE_ON_MOUSE_MOVE_AWAY: u64 = 8;
pub const KEEP_ON_SELECTION_MODIFIED: u64 = 16;

/// Sublime's hover zones (`on_hover`): over the text, the gutter, or the margin after a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverZone {
    Text = 1,
    Gutter = 2,
    Margin = 3,
}

/// A popup a plugin shows (`view.show_popup`).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginPopup {
    pub view: DocumentId,
    /// The char it points at (the caret when the plugin passed -1).
    pub location: usize,
    /// minihtml.
    pub content: String,
    /// In pixels, as Sublime's.
    pub max_width: f32,
    pub max_height: f32,
    /// Sublime's `PopupFlags`.
    pub flags: u64,
    /// The plugin's callbacks (`navigate`, `hide`), if it gave any.
    callback: Option<u64>,
    /// The view's selections when it was shown: a change hides it.
    selections: Vec<Selection>,
}

impl Editor {
    pub fn plugin_popup(&self) -> Option<&PluginPopup> {
        self.state.plugins.popup.as_ref()
    }

    /// `view.show_popup`: replaces a popup already shown (whose `on_hide` runs).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn show_plugin_popup(
        &mut self,
        view: DocumentId,
        content: &str,
        flags: u64,
        location: i64,
        max_width: f32,
        max_height: f32,
        callback: Option<u64>,
    ) -> Result<(), String> {
        let doc = self.any_document(view).ok_or("the view is closed")?;
        let len = doc.char_count();
        let location = usize::try_from(location).map_or(doc.selection().head, |l| l.min(len));
        let selections = std::iter::once(doc.selection()).chain(doc.extra_selections().iter().copied()).collect();
        self.hide_plugin_popup();
        self.state.plugins.popup = Some(PluginPopup {
            view,
            location,
            content: content.to_string(),
            max_width: if max_width > 0.0 { max_width } else { 320.0 },
            max_height: if max_height > 0.0 { max_height } else { 240.0 },
            flags,
            callback,
            selections,
        });
        Ok(())
    }

    /// `view.update_popup`: new contents for the popup of that view, if it has one.
    pub(crate) fn update_plugin_popup(&mut self, view: DocumentId, content: &str) {
        if let Some(popup) = self.state.plugins.popup.as_mut().filter(|p| p.view == view) {
            popup.content = content.to_string();
        }
    }

    pub(crate) fn plugin_popup_visible(&self, view: DocumentId) -> bool {
        self.state.plugins.popup.as_ref().is_some_and(|p| p.view == view)
    }

    /// Hides the popup (Esc, the mouse moved away, a plugin's `hide_popup`); its `on_hide` runs
    /// at the next [`Editor::poll_plugins`].
    pub fn hide_plugin_popup(&mut self) {
        if let Some(popup) = self.state.plugins.popup.take() {
            if let Some(callback) = popup.callback {
                self.state.plugins.pending_callbacks.push((callback, "hide".into(), json!([])));
            }
        }
    }

    /// A link was clicked in the popup: its `on_navigate(href)`.
    pub fn navigate_plugin_popup(&mut self, href: &str, run_command: &mut CommandRunner) {
        let Some(callback) = self.state.plugins.popup.as_ref().and_then(|p| p.callback) else {
            return;
        };
        self.plugin_callback(callback, "navigate", json!([href]), run_command);
    }

    /// A link was clicked in a phantom: its `on_navigate(href)`.
    pub fn navigate_plugin_phantom(&mut self, phantom: u64, href: &str, run_command: &mut CommandRunner) {
        if let Some(&callback) = self.state.plugins.phantom_callbacks.get(&phantom) {
            self.plugin_callback(callback, "navigate", json!([href]), run_command);
        }
    }

    /// A link was clicked in an annotation of the region set `key`: its `on_navigate(href)`.
    pub fn navigate_plugin_annotation(&mut self, view: DocumentId, key: &str, href: &str, run_command: &mut CommandRunner) {
        if let Some(&callback) = self.state.plugins.region_callbacks.get(&(view, key.to_string())) {
            self.plugin_callback(callback, "navigate", json!([href]), run_command);
        }
    }

    /// The popup goes when its view is no longer active or its caret moved (see the module).
    pub(crate) fn hide_popup_if_moved_away(&mut self) {
        let Some(popup) = &self.state.plugins.popup else {
            return;
        };
        let active = self.active_document().map(|d| d.id);
        let in_panel = active != Some(popup.view) && self.any_document(popup.view).is_some() && self.index_of(popup.view).is_none();
        let gone = match self.any_document(popup.view) {
            None => true,
            Some(_) if in_panel => false,
            Some(_) if active != Some(popup.view) => true,
            Some(doc) => {
                let now: Vec<Selection> = std::iter::once(doc.selection()).chain(doc.extra_selections().iter().copied()).collect();
                popup.flags & KEEP_ON_SELECTION_MODIFIED == 0 && now != popup.selections
            }
        };
        if gone {
            self.hide_plugin_popup();
        }
    }

    /// `view.add_phantom`: its id.
    pub(crate) fn add_plugin_phantom(
        &mut self,
        view: DocumentId,
        key: &str,
        region: (usize, usize),
        content: &str,
        layout: u64,
        callback: Option<u64>,
    ) -> Result<u64, String> {
        self.state.plugins.next_phantom_id += 1;
        let id = self.state.plugins.next_phantom_id;
        let doc = self.any_document_mut(view).ok_or("the view is closed")?;
        doc.add_phantom(Phantom { id, key: key.to_string(), region, content: content.to_string(), layout });
        if let Some(callback) = callback {
            self.state.plugins.phantom_callbacks.insert(id, callback);
        }
        Ok(id)
    }

    pub(crate) fn erase_plugin_phantoms(&mut self, view: DocumentId, key: &str) {
        let Some(doc) = self.any_document_mut(view) else { return };
        let gone: Vec<u64> = doc.phantoms().iter().filter(|p| p.key == key).map(|p| p.id).collect();
        doc.erase_phantoms(key);
        for id in gone {
            self.state.plugins.phantom_callbacks.remove(&id);
        }
    }

    pub(crate) fn erase_plugin_phantom(&mut self, view: DocumentId, id: u64) {
        if let Some(doc) = self.any_document_mut(view) {
            doc.erase_phantom(id);
        }
        self.state.plugins.phantom_callbacks.remove(&id);
    }

    /// `view.query_phantom`: `[[a, b]]`, or `[]` when it is gone.
    pub(crate) fn query_plugin_phantom(&self, view: DocumentId, id: u64) -> Value {
        let phantoms = self.any_document(view).map(|d| d.phantoms()).unwrap_or_default();
        json!(phantoms.iter().filter(|p| p.id == id).map(|p| [p.region.0, p.region.1]).collect::<Vec<_>>())
    }

    /// The mouse rests over a view: `on_hover(view, point, hover_zone)`.
    pub fn plugin_hover(&mut self, view: DocumentId, point: usize, zone: HoverZone) {
        self.plugin_query("on_hover", Some(view), json!([point, zone as u8]));
    }

    /// Some listener has `on_hover` (the view need not tell otherwise).
    pub fn plugins_hear_hover(&self) -> bool {
        self.wants_plugin_event("on_hover")
    }
}
