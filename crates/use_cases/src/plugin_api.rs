//! Use cases: the `sublime_api` functions plugins call (phase A of plugin support).
//!
//! Each function of `assets/python/sublime.py` that needs the editor ends up here by name, with
//! JSON arguments. Positions are char indices, like [`Selection`]. View ids are document ids;
//! there is a single window. Functions not listed here answer "not implemented yet"; the next
//! ones to add are listed in `docs/plugins.md`.

use entities::build::expand_variables;
use entities::command::Command;
use entities::document::{DocumentId, LineEnding, RegionSet};
use entities::find::{self, FindOptions};
use entities::selection::Selection;
use entities::text::{byte_to_char, char_to_byte, line_col, line_end, line_start, line_to_char, selected_text, word_at};
use serde_json::{json, Map, Value};

use crate::editor::Editor;
use crate::gateways::{FileFilter, MessageAnswer, MessageButtons, MessageDialog, MessageLevel};
use crate::plugins::CommandRunner;

const PREFERENCES: &str = "Preferences.sublime-settings";
/// The title of plugins' message dialogs (Sublime's say "Sublime Text").
const APP_NAME: &str = "CodeLibre Text";
/// `sublime.LITERAL` and `sublime.IGNORECASE`.
const LITERAL: u64 = 1;
const IGNORECASE: u64 = 2;

/// The arguments of one call, with errors that name the bad argument.
struct Args<'a>(&'a [Value]);

impl Args<'_> {
    fn value(&self, i: usize) -> &Value {
        self.0.get(i).unwrap_or(&Value::Null)
    }

    fn str(&self, i: usize) -> Result<&str, String> {
        self.value(i).as_str().ok_or_else(|| format!("argument {} must be a string", i + 1))
    }

    fn u64(&self, i: usize) -> Result<u64, String> {
        match self.value(i) {
            Value::Number(n) => n.as_u64().or_else(|| n.as_i64().map(|v| v.max(0) as u64)),
            _ => None,
        }
        .ok_or_else(|| format!("argument {} must be a number", i + 1))
    }

    fn usize(&self, i: usize) -> Result<usize, String> {
        self.u64(i).map(|v| v as usize)
    }

    fn bool(&self, i: usize) -> bool {
        self.value(i).as_bool().unwrap_or(false)
    }
}

/// Which settings object a plugin is talking to.
enum SettingsTarget {
    View(DocumentId),
    File(String),
    Window,
}

fn settings_target(value: &Value) -> Result<SettingsTarget, String> {
    match (value.get(0).and_then(Value::as_str), value.get(1)) {
        (Some("view"), Some(id)) => id.as_u64().map(SettingsTarget::View).ok_or_else(|| "bad view id".into()),
        (Some("file"), Some(Value::String(name))) => Ok(SettingsTarget::File(name.clone())),
        (Some("window"), _) => Ok(SettingsTarget::Window),
        _ => Err(format!("unknown settings object {value}")),
    }
}

/// Sublime's `file_types`: `[(description, [extension, …]), …]`.
fn file_filters(value: &Value) -> Vec<FileFilter> {
    let filter = |entry: &Value| {
        let name = entry.get(0)?.as_str()?.to_string();
        let extensions = entry.get(1)?.as_array()?.iter().filter_map(|e| e.as_str().map(String::from)).collect();
        Some(FileFilter { name, extensions })
    };
    value.as_array().map(|entries| entries.iter().filter_map(filter).collect()).unwrap_or_default()
}

fn region(a: usize, b: usize) -> Value {
    json!([a, b])
}

impl Editor {
    /// Runs one `sublime_api` function for a plugin.
    pub(crate) fn plugin_call(&mut self, function: &str, args: &[Value], run_command: &mut CommandRunner) -> Result<Value, String> {
        let a = Args(args);
        match function {
            "status_message" => {
                self.notify(a.str(0)?);
                Ok(Value::Null)
            }
            "error_message" | "message_dialog" => {
                let message = a.str(0)?.to_string();
                self.log(&format!("{message}\n"));
                let level = if function == "error_message" { MessageLevel::Error } else { MessageLevel::Info };
                self.plugin_dialog(level, "", &message, MessageButtons::Ok);
                Ok(Value::Null)
            }
            "ok_cancel_dialog" => {
                let ok = Self::button_label(a.str(1).unwrap_or_default(), self.tr("OK"));
                let buttons = MessageButtons::OkCancel { ok, cancel: self.tr("Cancel").to_string() };
                let answer = self.plugin_dialog(MessageLevel::Info, a.str(2).unwrap_or_default(), a.str(0)?, buttons);
                Ok(json!(answer == MessageAnswer::Ok))
            }
            "yes_no_cancel_dialog" => {
                let buttons = MessageButtons::YesNoCancel {
                    yes: Self::button_label(a.str(1).unwrap_or_default(), self.tr("Yes")),
                    no: Self::button_label(a.str(2).unwrap_or_default(), self.tr("No")),
                    cancel: self.tr("Cancel").to_string(),
                };
                let answer = self.plugin_dialog(MessageLevel::Info, a.str(3).unwrap_or_default(), a.str(0)?, buttons);
                // `sublime.DIALOG_CANCEL`, `DIALOG_YES`, `DIALOG_NO`.
                Ok(json!(match answer {
                    MessageAnswer::Cancel => 0,
                    MessageAnswer::Ok => 1,
                    MessageAnswer::No => 2,
                }))
            }
            "open_dialog" => {
                let files = self.gateways.dialogs.pick_files(self.dialog_directory(a.value(0)).as_deref(), &file_filters(a.value(1)));
                let files: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
                Ok(if files.is_empty() { Value::Null } else { json!(files) })
            }
            "save_dialog" => {
                let name = a.str(1).unwrap_or_default();
                let extension = a.str(2).unwrap_or_default();
                let file_name = match (name, extension) {
                    (name, "") => name.to_string(),
                    (name, extension) => format!("{name}.{extension}"),
                };
                let filters = file_filters(a.value(3));
                let path = self.gateways.dialogs.pick_save_path(self.dialog_directory(a.value(0)).as_deref(), &file_name, &filters);
                Ok(path.map_or(Value::Null, |p| json!(p.display().to_string())))
            }
            "select_folder_dialog" => {
                let start = self.dialog_directory(a.value(0));
                let folders = if a.bool(1) {
                    self.gateways.dialogs.pick_folders(start.as_deref())
                } else {
                    self.gateways.dialogs.pick_folder(start.as_deref()).into_iter().collect()
                };
                let folders: Vec<String> = folders.iter().map(|p| p.display().to_string()).collect();
                Ok(if folders.is_empty() { Value::Null } else { json!(folders) })
            }
            "expand_variables" => {
                let variables = a.value(1).as_object().into_iter().flatten();
                let variables = variables.filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_string()))).collect();
                Ok(json!(expand_variables(a.str(0)?, &variables)))
            }
            "find_resources" => {
                let include_sublime = self.global_settings().bool("import_sublime_packages", true);
                Ok(json!(self.gateways.resources.find_resources(a.str(0)?, include_sublime)))
            }
            "load_resource" | "load_binary_resource" => {
                let include_sublime = self.global_settings().bool("import_sublime_packages", true);
                let name = a.str(0)?;
                let bytes = self.gateways.resources.load_resource(name, include_sublime)?;
                if function == "load_resource" {
                    // Sublime gives resources `\n` line endings.
                    let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not UTF-8 text"))?;
                    Ok(json!(text.replace("\r\n", "\n")))
                } else {
                    // JSON has no bytes: hexadecimal, read with `bytes.fromhex`.
                    Ok(json!(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()))
                }
            }
            "get_clipboard" => self.gateways.clipboard.read_text().map(Value::from),
            "set_clipboard" => self.gateways.clipboard.write_text(a.str(0)?).map(|()| Value::Null),
            "run_command" => {
                let command = Command::with_args(a.str(2)?, a.value(3).clone());
                self.plugin_run_command(a.str(0)?, a.u64(1)?, &command, run_command)
            }
            "settings_get" => self.plugin_settings_get(&settings_target(a.value(0))?, a.str(1)?),
            "settings_set" => self.plugin_settings_set(a.value(0), a.str(1)?, a.value(2).clone()),
            "settings_erase" => self.plugin_settings_set(a.value(0), a.str(1)?, Value::Null),
            "settings_watch" => self.plugin_settings_watch(a.value(0)),
            "settings_all" => Ok(Value::Object(self.plugin_settings_all(&settings_target(a.value(0))?))),
            "save_settings" => self.plugin_save_settings(a.str(0)?),
            "window_active_view" => Ok(self.active_document().map_or(Value::Null, |d| json!(d.id))),
            "window_views" => Ok(json!(self.state.documents.iter().map(|d| d.id).collect::<Vec<_>>())),
            "window_new_file" => {
                self.new_file();
                Ok(self.active_document().map_or(Value::Null, |d| json!(d.id)))
            }
            "window_open_file" => {
                let opened = self.open_file(std::path::Path::new(a.str(0)?));
                Ok(if opened { self.active_document().map_or(Value::Null, |d| json!(d.id)) } else { Value::Null })
            }
            "window_find_open_file" => {
                let path = self.gateways.file_system.canonicalize(std::path::Path::new(a.str(0)?));
                let doc = self.state.documents.iter().find(|d| d.path.as_deref() == Some(path.as_path()));
                Ok(doc.map_or(Value::Null, |d| json!(d.id)))
            }
            "window_focus_view" => {
                self.activate_document(a.u64(0)?);
                Ok(Value::Null)
            }
            "window_show_quick_panel" => self.plugin_show_quick_panel(args, run_command),
            "window_show_input_panel" => self.plugin_show_input_panel(args, run_command),
            "window_folders" => Ok(json!(self
                .state
                .folders
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>())),
            "view_is_valid" => Ok(json!(self.any_document(a.u64(0)?).is_some())),
            "view_find_all_results" => {
                let results = self.output_results(a.u64(0)?);
                Ok(json!(results.iter().map(|r| json!([r.file, r.row, r.column, r.message])).collect::<Vec<_>>()))
            }
            "view_set_status" | "view_erase_status" => {
                let id = a.u64(0)?;
                self.any_document(id).ok_or("the view is closed")?;
                let statuses = self.state.plugins.view_statuses.entry(id).or_default();
                if function == "view_set_status" {
                    statuses.insert(a.str(1)?.to_string(), a.str(2)?.to_string());
                } else {
                    statuses.remove(a.str(1)?);
                }
                Ok(Value::Null)
            }
            "view_get_status" => {
                let statuses = self.state.plugins.view_statuses.get(&a.u64(0)?);
                Ok(json!(statuses.and_then(|s| s.get(a.str(1).unwrap_or_default())).cloned().unwrap_or_default()))
            }
            "window_create_output_panel" => Ok(json!(self.create_output_panel(a.str(1)?))),
            "window_find_output_panel" => Ok(self.find_output_panel(a.str(1)?).map_or(Value::Null, |id| json!(id))),
            "window_destroy_output_panel" => {
                self.destroy_output_panel(a.str(1)?);
                Ok(Value::Null)
            }
            "window_active_panel" if self.plugin_input_panel().is_some() => Ok(json!("input")),
            "window_active_panel" => Ok(self.shown_output_panel().map_or(Value::Null, |p| json!(format!("output.{}", p.name)))),
            "window_panels" => Ok(json!(self.output_panel_names())),
            "view_close" => {
                // A dirty view asks to save first, so it may still be open.
                let id = a.u64(0)?;
                self.close_documents(vec![id]);
                Ok(json!(self.index_of(id).is_none()))
            }
            _ if function.starts_with("view_") => self.plugin_view_call(function, &a),
            _ => Err(format!("sublime_api.{function} is not implemented yet")),
        }
    }

    fn plugin_view_call(&mut self, function: &str, a: &Args) -> Result<Value, String> {
        // Resolved before borrowing the document: it needs the syntax gateway.
        let syntax = match function {
            "view_assign_syntax" => Some(self.gateways.syntaxes.resolve(a.str(1)?)),
            _ => None,
        };
        let doc = self.any_document_mut(a.u64(0)?).ok_or("the view is closed")?;
        let len = doc.char_count();
        let point = |i: usize| a.usize(i).map(|p| p.min(len));
        Ok(match function {
            "view_file_name" => doc.path.as_ref().map_or(Value::Null, |p| json!(p.display().to_string())),
            "view_name" => json!(doc.scratch_name.clone().unwrap_or_default()),
            "view_set_name" => {
                doc.scratch_name = Some(a.str(1)?.to_string());
                Value::Null
            }
            "view_is_dirty" => json!(doc.is_dirty()),
            "view_is_scratch" => json!(doc.scratch),
            "view_set_scratch" => {
                doc.scratch = a.bool(1);
                Value::Null
            }
            "view_change_count" => json!(doc.change_count()),
            "view_is_read_only" => json!(doc.read_only),
            "view_set_read_only" => {
                doc.read_only = a.bool(1);
                Value::Null
            }
            "view_encoding" => json!(doc.encoding.label()),
            "view_line_endings" => json!(doc.line_ending.label()),
            "view_set_line_endings" => {
                doc.line_ending = LineEnding::from_setting(Some(&a.str(1)?.to_lowercase()), doc.line_ending);
                Value::Null
            }
            "view_syntax" => json!(doc.syntax),
            "view_assign_syntax" => {
                doc.syntax = syntax.unwrap_or_default();
                Value::Null
            }
            "view_size" => json!(len),
            "view_visible_region" => {
                let (start, end) = doc.visible_range();
                region(start, end)
            }
            "view_add_regions" => {
                let regions = a
                    .value(2)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| Some((r.get(0)?.as_u64()? as usize, r.get(1)?.as_u64()? as usize)))
                    .collect();
                doc.add_regions(RegionSet {
                    key: a.str(1)?.to_string(),
                    regions,
                    scope: a.str(3).unwrap_or_default().to_string(),
                    icon: a.str(4).unwrap_or_default().to_string(),
                    flags: a.u64(5).unwrap_or(0),
                });
                Value::Null
            }
            "view_get_regions" => json!(doc.regions(a.str(1)?).iter().map(|&(start, end)| [start, end]).collect::<Vec<_>>()),
            "view_erase_regions" => {
                doc.erase_regions(a.str(1)?);
                Value::Null
            }
            "view_show" => {
                doc.request_show(point(1)?, a.bool(2));
                Value::Null
            }
            "view_cached_substr" => json!(selected_text(&doc.text, Selection::new(point(1)?, point(2)?))),
            "view_insert" => {
                let text = a.str(2)?;
                json!(if doc.replace_range(point(1)?, point(1)?, text) { text.chars().count() } else { 0 })
            }
            "view_erase" => {
                doc.replace_range(point(1)?, point(2)?, "");
                Value::Null
            }
            "view_replace" => {
                doc.replace_range(point(1)?, point(2)?, a.str(3)?);
                Value::Null
            }
            "view_selection_get" => json!(doc.selections().iter().map(|s| [s.anchor, s.head]).collect::<Vec<_>>()),
            "view_selection_set" => {
                let mut regions: Vec<Selection> = a
                    .value(1)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| Some(Selection::new(r.get(0)?.as_u64()? as usize, r.get(1)?.as_u64()? as usize)))
                    .collect();
                // The last region is the primary one, so the view follows it.
                if let Some(primary) = regions.pop() {
                    doc.set_selections(primary, regions);
                }
                Value::Null
            }
            "view_line" | "view_full_line" => {
                let (start, end) = (char_to_byte(&doc.text, point(1)?), char_to_byte(&doc.text, point(2)?));
                let mut line = (line_start(&doc.text, start), line_end(&doc.text, end));
                if function == "view_full_line" && line.1 < doc.text.len() {
                    line.1 += 1;
                }
                region(byte_to_char(&doc.text, line.0), byte_to_char(&doc.text, line.1))
            }
            "view_lines" => {
                let text = &doc.text;
                let end = char_to_byte(text, point(2)?);
                let mut pos = line_start(text, char_to_byte(text, point(1)?));
                let mut lines = Vec::new();
                loop {
                    let le = line_end(text, pos);
                    lines.push(region(byte_to_char(text, pos), byte_to_char(text, le)));
                    if le >= end || le >= text.len() {
                        break;
                    }
                    pos = le + 1;
                }
                Value::Array(lines)
            }
            "view_word" => region(word_at(&doc.text, point(1)?).start(), word_at(&doc.text, point(2)?).end()),
            "view_rowcol" => {
                let (row, col) = line_col(&doc.text, point(1)?);
                json!([row, col])
            }
            "view_text_point" => json!((line_to_char(&doc.text, a.usize(1)?) + a.usize(2)?).min(len)),
            "view_find" | "view_find_all" => {
                let flags = a.u64(if function == "view_find" { 3 } else { 2 }).unwrap_or(0);
                let options = FindOptions {
                    query: a.str(1)?.to_string(),
                    regex: flags & LITERAL == 0,
                    case_sensitive: flags & IGNORECASE == 0,
                    whole_word: false,
                    wrap: false,
                };
                let matches = match options.pattern()? {
                    Some(pattern) => find::find_all_selections(&pattern, &doc.text),
                    None => Vec::new(),
                };
                if function == "view_find" {
                    let start = point(2)?;
                    let found = matches.into_iter().find(|m| m.start() >= start);
                    found.map_or(json!([-1, -1]), |m| region(m.start(), m.end()))
                } else {
                    Value::Array(matches.iter().map(|m| region(m.start(), m.end())).collect())
                }
            }
            _ => return Err(format!("sublime_api.{function} is not implemented yet")),
        })
    }

    /// A modal message for a plugin, titled with the application's name unless the plugin gives one.
    fn plugin_dialog(&self, level: MessageLevel, title: &str, text: &str, buttons: MessageButtons) -> MessageAnswer {
        let title = if title.is_empty() { APP_NAME } else { title };
        let dialog = MessageDialog { level, title: title.to_string(), text: text.to_string(), buttons };
        self.gateways.dialogs.show_message(&dialog)
    }

    /// A button's label: the plugin's, or the (translated) default.
    fn button_label(label: &str, default: &str) -> String {
        if label.is_empty() { default } else { label }.to_string()
    }

    /// The folder a plugin's file dialog starts in, or the usual one.
    fn dialog_directory(&self, directory: &Value) -> Option<std::path::PathBuf> {
        self.dialog_start(directory.as_str())
    }

    /// `view.run_command`, `window.run_command` or `sublime.run_command`. A view's command runs
    /// with that view active; the previously active view comes back afterwards.
    fn plugin_run_command(
        &mut self,
        kind: &str,
        target: DocumentId,
        command: &Command,
        run_command: &mut CommandRunner,
    ) -> Result<Value, String> {
        if command.name == "append" {
            let id = if kind == "view" { target } else { self.active_document().ok_or("no view is open")?.id };
            self.append_text(id, command.str_arg("characters"), command.bool_arg("force", false))?;
            return Ok(Value::Null);
        }
        if kind == "view" && self.is_panel_view(target) {
            if self.run_plugin_command_in(command, Some(target), run_command) {
                return Ok(Value::Null);
            }
            return self.run_in_panel(target, command);
        }
        let previous = self.state.active;
        if kind == "view" {
            self.state.active = self.index_of(target).ok_or("the view is closed")?;
        }
        if !self.run_plugin_command(command, run_command) {
            run_command(self, command);
        }
        if kind == "view" && self.active_document().is_some_and(|d| d.id == target) && previous < self.state.documents.len() {
            self.state.active = previous;
        }
        Ok(Value::Null)
    }

    /// The built-in commands that work in a panel's view (the command controller only knows tabs).
    fn run_in_panel(&mut self, id: DocumentId, command: &Command) -> Result<Value, String> {
        let doc = self.any_document_mut(id).ok_or("the view is closed")?;
        match command.name.as_str() {
            "select_all" => doc.set_selection(Selection::new(0, doc.char_count())),
            "insert" => {
                let selection = doc.selection();
                doc.replace_range(selection.start(), selection.end(), command.str_arg("characters"));
            }
            other => return Err(format!("{other} cannot run in a panel yet (only append, insert, select_all and plugin commands)")),
        }
        Ok(Value::Null)
    }

    fn plugin_settings_get(&mut self, target: &SettingsTarget, key: &str) -> Result<Value, String> {
        let all = self.plugin_settings_all(target);
        Ok(match all.get(key) {
            Some(value) => json!([true, value]),
            None => json!([false, null]),
        })
    }

    fn plugin_settings_all(&mut self, target: &SettingsTarget) -> Map<String, Value> {
        match target {
            SettingsTarget::View(id) => self.settings_of(*id).values().clone(),
            SettingsTarget::File(name) if name == PREFERENCES => self.global_settings().values().clone(),
            SettingsTarget::File(name) => self.settings_file(name).values.clone(),
            SettingsTarget::Window => self.state.plugins.window_settings.clone(),
        }
    }

    /// `add_on_change`: the editor tells the plugins (`settings_changed`) when this object changes.
    fn plugin_settings_watch(&mut self, target: &Value) -> Result<Value, String> {
        let values = self.plugin_settings_all(&settings_target(target)?);
        let watched = &mut self.state.plugins.watched_settings;
        if !watched.iter().any(|(t, _)| t == target) {
            watched.push((target.clone(), values));
        }
        Ok(Value::Null)
    }

    /// Tells the plugins which watched settings objects changed since the last check: any way the
    /// editor changes them (`set_setting`, the user saving a settings file…) ends up here.
    pub(crate) fn notice_settings_changes(&mut self) {
        let mut watched = std::mem::take(&mut self.state.plugins.watched_settings);
        let mut changed = Vec::new();
        for (target, before) in &mut watched {
            let Ok(object) = settings_target(target) else { continue };
            let now = self.plugin_settings_all(&object);
            if now != *before {
                *before = now;
                changed.push(target.clone());
            }
        }
        self.state.plugins.watched_settings = watched;
        for target in changed {
            self.ask_plugins("settings_changed", None, json!([target]));
        }
    }

    /// Sets a setting; `Value::Null` erases it. The plugin's side calls its own `add_on_change`
    /// callbacks, so the object's remembered values move on too.
    fn plugin_settings_set(&mut self, target: &Value, key: &str, value: Value) -> Result<Value, String> {
        let object = settings_target(target)?;
        self.set_plugin_setting(&object, key, value);
        let now = self.plugin_settings_all(&object);
        if let Some((_, values)) = self.state.plugins.watched_settings.iter_mut().find(|(t, _)| t == target) {
            *values = now;
        }
        Ok(Value::Null)
    }

    fn set_plugin_setting(&mut self, target: &SettingsTarget, key: &str, value: Value) {
        let set = |map: &mut Map<String, Value>, value: Value| {
            if value.is_null() {
                map.remove(key);
            } else {
                map.insert(key.to_string(), value);
            }
        };
        match target {
            SettingsTarget::View(id) => set(self.state.plugins.view_settings.entry(*id).or_default(), value),
            SettingsTarget::File(name) => {
                if name == PREFERENCES {
                    if value.is_null() {
                        self.state.settings.remove_override(key);
                    } else {
                        self.set_setting(key, value.clone());
                    }
                }
                let file = self.settings_file(name);
                file.changes.insert(key.to_string(), value.clone());
                set(&mut file.values, value);
            }
            SettingsTarget::Window => set(&mut self.state.plugins.window_settings, value),
        }
    }

    /// `sublime.save_settings`: writes what plugins changed to the `User` package's copy.
    fn plugin_save_settings(&mut self, name: &str) -> Result<Value, String> {
        let changes = std::mem::take(&mut self.settings_file(name).changes);
        if name == PREFERENCES {
            for (key, value) in changes {
                self.persist_setting(&key, value);
            }
            return Ok(Value::Null);
        }
        let mut user = self.gateways.resources.read_user_settings_file(name);
        for (key, value) in changes {
            if value.is_null() {
                user.remove(&key);
            } else {
                user.insert(key, value);
            }
        }
        self.gateways.resources.write_user_settings_file(name, &user).map(|()| Value::Null)
    }

    /// A settings file as plugins see it (read from the packages the first time).
    fn settings_file(&mut self, name: &str) -> &mut crate::plugins::SettingsFile {
        if !self.state.plugins.settings_files.contains_key(name) {
            let include_sublime = self.global_settings().bool("import_sublime_packages", true);
            let mut errors = Vec::new();
            let values = if name == PREFERENCES {
                Map::new()
            } else {
                self.gateways.resources.load_settings_file(name, include_sublime, &mut errors)
            };
            errors.iter().for_each(|e| self.log(&format!("{e}\n")));
            self.state.plugins.settings_files.insert(
                name.to_string(),
                crate::plugins::SettingsFile { values, changes: Map::new() },
            );
        }
        self.state.plugins.settings_files.get_mut(name).expect("inserted above")
    }
}
