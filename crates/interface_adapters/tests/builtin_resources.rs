//! Tests for the built-in resources in `assets/Default/` (compiled into the binary).

use interface_adapters::gateways::{builtin_resources, color_scheme_file, po_file, sublime_json};
use serde_json::Value;

#[test]
fn every_builtin_json_resource_is_valid() {
    for (name, source) in [
        ("Default.sublime-keymap", builtin_resources::KEYMAP),
        ("Default.sublime-commands", builtin_resources::PALETTE_COMMANDS),
        ("Main.sublime-menu", builtin_resources::MAIN_MENU),
        ("Preferences.sublime-settings", builtin_resources::PREFERENCES),
    ] {
        sublime_json::parse(source).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn the_default_preferences_cover_the_documented_settings() {
    let preferences = sublime_json::parse(builtin_resources::PREFERENCES).unwrap();
    for key in [
        "color_scheme", "font_size", "tab_size", "translate_tabs_to_spaces", "auto_indent", "word_wrap",
        "line_numbers", "highlight_line", "rulers", "hot_exit", "import_sublime_packages",
        "folder_exclude_patterns", "file_exclude_patterns",
    ] {
        assert!(preferences.get(key).is_some(), "missing default for {key}");
    }
}

#[test]
fn every_keymap_entry_has_keys_and_a_command() {
    let keymap = sublime_json::parse(builtin_resources::KEYMAP).unwrap();
    for binding in keymap.as_array().unwrap() {
        assert!(binding["keys"].as_array().is_some_and(|k| !k.is_empty()), "{binding}");
        assert!(binding["command"].is_string(), "{binding}");
    }
}

#[test]
fn every_palette_entry_has_a_caption_and_a_command() {
    let palette = sublime_json::parse(builtin_resources::PALETTE_COMMANDS).unwrap();
    for entry in palette.as_array().unwrap() {
        assert!(entry["caption"].is_string() && entry["command"].is_string(), "{entry}");
    }
}

#[test]
fn the_main_menu_has_the_sublime_top_level_menus() {
    let menu = sublime_json::parse(builtin_resources::MAIN_MENU).unwrap();
    let ids: Vec<&str> = menu.as_array().unwrap().iter().filter_map(|m| m["id"].as_str()).collect();
    assert_eq!(ids, vec!["file", "edit", "selection", "find", "view", "goto", "tools", "preferences", "help"]);
}

#[test]
fn the_builtin_color_schemes_define_the_editor_colors() {
    for (name, source) in builtin_resources::COLOR_SCHEMES {
        let scheme = color_scheme_file::parse(source, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        for global in ["background", "foreground", "caret", "selection", "line_highlight", "gutter_foreground"] {
            assert!(scheme.globals.contains_key(global), "{name} has no {global}");
        }
        assert!(scheme.rules.len() > 10, "{name} has rules");
    }
}

/// Every `"command"` value anywhere in a resource.
pub fn commands_in(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(command)) = map.get("command") {
                out.push(command.clone());
            }
            map.values().for_each(|v| commands_in(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| commands_in(v, out)),
        _ => {}
    }
}

#[test]
fn the_builtin_resources_reference_commands() {
    let mut commands = Vec::new();
    for source in [builtin_resources::KEYMAP, builtin_resources::MAIN_MENU, builtin_resources::PALETTE_COMMANDS] {
        commands_in(&sublime_json::parse(source).unwrap(), &mut commands);
    }
    assert!(commands.len() > 100);
}

// ----- translations ------------------------------------------------------------------------------

/// A text to translate: its English singular, the plural if it has one, and where it is used.
struct Message {
    msgid: String,
    plural: Option<String>,
    origins: Vec<String>,
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn add_message(messages: &mut Vec<Message>, msgid: String, plural: Option<String>, origin: &str) {
    match messages.iter_mut().find(|m| m.msgid == msgid) {
        Some(message) if !message.origins.iter().any(|o| o == origin) => message.origins.push(origin.to_string()),
        Some(_) => {}
        None => messages.push(Message { msgid, plural, origins: vec![origin.to_string()] }),
    }
}

/// Every caption of the built-in menu and command palette, and every literal passed to
/// `tr`, `tr_args` and `tr_n` in the application's sources.
fn translatable_messages() -> Vec<Message> {
    fn captions(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(caption)) = map.get("caption") {
                    if caption != "-" {
                        out.push(caption.replace('&', ""));
                    }
                }
                if let Some(Value::Array(children)) = map.get("children") {
                    children.iter().for_each(|v| captions(v, out));
                }
            }
            Value::Array(items) => items.iter().for_each(|v| captions(v, out)),
            _ => {}
        }
    }
    let mut messages = Vec::new();
    for (origin, source) in [
        ("assets/Default/Main.sublime-menu", builtin_resources::MAIN_MENU),
        ("assets/Default/Default.sublime-commands", builtin_resources::PALETTE_COMMANDS),
    ] {
        let mut found = Vec::new();
        captions(&sublime_json::parse(source).unwrap(), &mut found);
        for caption in found {
            add_message(&mut messages, caption, None, origin);
        }
    }

    let literal = r#""((?:[^"\\]|\\.)*)""#;
    let single = regex::Regex::new(&format!(r"\btr(?:_args)?\(\s*{literal}")).unwrap();
    let plural = regex::Regex::new(&format!(r"\btr_n\(\s*{literal}\s*,\s*{literal}")).unwrap();
    let unescape = |s: &str| s.replace(r"\n", "\n").replace(r#"\""#, "\"").replace(r"\'", "'").replace(r"\\", "\\");
    let root = repository_root();
    for krate in ["use_cases", "interface_adapters", "frameworks"] {
        let mut files = Vec::new();
        let mut dirs = vec![root.join("crates").join(krate).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            let origin = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            let mut found: Vec<(usize, String, Option<String>)> = single
                .captures_iter(&source)
                .map(|c| (c.get(0).unwrap().start(), unescape(&c[1]), None))
                .chain(
                    plural
                        .captures_iter(&source)
                        .map(|c| (c.get(0).unwrap().start(), unescape(&c[1]), Some(unescape(&c[2])))),
                )
                .collect();
            found.sort_by_key(|(at, ..)| *at);
            for (_, msgid, plural) in found {
                add_message(&mut messages, msgid, plural, &origin);
            }
        }
    }
    messages
}

fn po_string(text: &str) -> String {
    let escaped = text.replace('\\', r"\\").replace('"', r#"\""#).replace('\n', r"\n");
    format!("\"{escaped}\"")
}

/// `messages.pot`, the template translators start from.
fn translation_template(messages: &[Message]) -> String {
    let mut pot = String::from(concat!(
        "# CodeLibre Text: template for translations of the interface.\n",
        "# Generated by `UPDATE_TRANSLATIONS=1 cargo test -p interface_adapters --test builtin_resources`;\n",
        "# do not edit. Translations live next to it as <language>.po (see docs/translating.md).\n",
        "msgid \"\"\n",
        "msgstr \"\"\n",
        "\"Content-Type: text/plain; charset=UTF-8\\n\"\n",
    ));
    for message in messages {
        pot.push('\n');
        for origin in &message.origins {
            pot.push_str(&format!("#: {origin}\n"));
        }
        pot.push_str(&format!("msgid {}\n", po_string(&message.msgid)));
        match &message.plural {
            Some(plural) => pot.push_str(&format!("msgid_plural {}\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n", po_string(plural))),
            None => pot.push_str("msgstr \"\"\n"),
        }
    }
    pot
}

#[test]
fn the_translation_template_lists_every_translatable_text() {
    let expected = translation_template(&translatable_messages());
    let path = repository_root().join("assets/Default/Localization/messages.pot");
    if std::env::var_os("UPDATE_TRANSLATIONS").is_some() {
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let actual = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    assert!(
        actual == expected,
        "assets/Default/Localization/messages.pot is out of date: run\n  \
         UPDATE_TRANSLATIONS=1 cargo test -p interface_adapters --test builtin_resources\n\
         and translate the new texts in every assets/Default/Localization/*.po"
    );
}

/// `{name}` placeholders of a text, sorted.
fn placeholders(text: &str) -> Vec<String> {
    let pattern = regex::Regex::new(r"\{(\w+)\}").unwrap();
    let mut names: Vec<String> = pattern.captures_iter(text).map(|c| c[1].to_string()).collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn the_builtin_translations_are_complete() {
    let messages = translatable_messages();
    for (code, source) in builtin_resources::TRANSLATIONS {
        let po = po_file::parse(source).unwrap_or_else(|e| panic!("{code}.po: {e}"));
        assert_eq!(po.language.as_deref(), Some(*code), "{code}.po: Language header");
        assert!(po.language_name.is_some(), "{code}.po: no X-Language-Name header");
        let catalog = &po.catalog;
        for message in &messages {
            let Some(translations) = catalog.translations(&message.msgid) else {
                panic!("{code}.po: no translation for {:?}", message.msgid);
            };
            let forms = if message.plural.is_some() { catalog.plural_forms() } else { 1 };
            assert_eq!(translations.len(), forms, "{code}.po: forms of {:?}", message.msgid);
            let mut expected = placeholders(&message.msgid);
            expected.extend(message.plural.iter().flat_map(|p| placeholders(p)));
            expected.sort();
            expected.dedup();
            for translation in translations {
                assert!(!translation.is_empty(), "{code}.po: empty translation of {:?}", message.msgid);
                // A plural form may leave `{count}` out (e.g. "one file" instead of "1 file").
                let mut found = placeholders(translation);
                let mut expected = expected.clone();
                if message.plural.is_some() {
                    expected.retain(|p| p != "count");
                    found.retain(|p| p != "count");
                }
                assert_eq!(found, expected, "{code}.po: placeholders of {:?}", message.msgid);
            }
        }
        for msgid in catalog.msgids() {
            assert!(messages.iter().any(|m| m.msgid == msgid), "{code}.po: {msgid:?} is not used any more");
        }
    }
}
