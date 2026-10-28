//! Use cases: the `sublime_api` functions plugins call (phase A of plugin support).
//!
//! Each function of `assets/python/sublime.py` that needs the editor ends up here by name, with
//! JSON arguments. Positions are char indices, like [`Selection`]. View ids are document ids;
//! there is a single window. Functions not listed here answer "not implemented yet"; the next
//! ones to add are listed in `docs/plugins.md`.

use entities::command::Command;
use entities::document::{DocumentId, LineEnding};
use entities::find::{self, FindOptions};
use entities::selection::Selection;
use entities::text::{byte_to_char, char_to_byte, line_col, line_end, line_start, line_to_char, selected_text, word_at};
use serde_json::{json, Map, Value};

use crate::editor::Editor;
use crate::plugins::CommandRunner;

const PREFERENCES: &str = "Preferences.sublime-settings";
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
                self.notify(message);
                Ok(Value::Null)
            }
            "get_clipboard" => self.gateways.clipboard.read_text().map(Value::from),
            "set_clipboard" => self.gateways.clipboard.write_text(a.str(0)?).map(|()| Value::Null),
            "run_command" => {
                let command = Command::with_args(a.str(2)?, a.value(3).clone());
                self.plugin_run_command(a.str(0)?, a.u64(1)?, &command, run_command)
            }
            "settings_get" => self.plugin_settings_get(&settings_target(a.value(0))?, a.str(1)?),
            "settings_set" => self.plugin_settings_set(&settings_target(a.value(0))?, a.str(1)?, a.value(2).clone()),
            "settings_erase" => self.plugin_settings_set(&settings_target(a.value(0))?, a.str(1)?, Value::Null),
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
            "view_is_valid" => Ok(json!(self.index_of(a.u64(0)?).is_some())),
            _ if function.starts_with("view_") => self.plugin_view_call(function, &a),
            _ => Err(format!("sublime_api.{function} is not implemented yet")),
        }
    }

    fn plugin_view_call(&mut self, function: &str, a: &Args) -> Result<Value, String> {
        let index = self.index_of(a.u64(0)?).ok_or("the view is closed")?;
        let doc = &mut self.state.documents[index];
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
                let syntax = self.gateways.syntaxes.resolve(a.str(1)?);
                self.state.documents[index].syntax = syntax;
                Value::Null
            }
            "view_size" => json!(len),
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

    /// `view.run_command`, `window.run_command` or `sublime.run_command`. A view's command runs
    /// with that view active; the previously active view comes back afterwards.
    fn plugin_run_command(
        &mut self,
        kind: &str,
        target: DocumentId,
        command: &Command,
        run_command: &mut CommandRunner,
    ) -> Result<Value, String> {
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

    /// Sets a setting; `Value::Null` erases it.
    fn plugin_settings_set(&mut self, target: &SettingsTarget, key: &str, value: Value) -> Result<Value, String> {
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
        Ok(Value::Null)
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
