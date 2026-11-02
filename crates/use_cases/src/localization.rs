//! Use cases: the interface language (the `language` setting) and translated messages.

use std::fmt::Display;
use std::rc::Rc;

use entities::document::{Document, UNTITLED};
use entities::localization::{resolve_language, Language, Translations, ENGLISH};
use serde_json::Value;

use crate::editor::Editor;

/// `language` setting value that follows the operating system.
pub const AUTOMATIC: &str = "auto";

impl Editor {
    /// Loads the translations for the `language` setting, unless they are already loaded.
    pub(crate) fn load_translations(&mut self, errors: &mut Vec<String>) {
        let include_sublime = self.global_settings().bool("import_sublime_packages", true);
        let setting = self.language_setting();
        let system = self.gateways.resources.system_language();
        let language = resolve_language(&setting, system.as_deref(), &self.languages());
        if language == ENGLISH {
            self.state.translations = Rc::default();
            return;
        }
        let catalog = self.gateways.resources.load_translations(&language, include_sublime, errors);
        self.state.translations = Rc::new(Translations::new(&language, catalog));
    }

    /// The `language` setting (`auto` by default).
    pub fn language_setting(&self) -> String {
        self.global_settings().str("language").unwrap_or(AUTOMATIC).to_string()
    }

    /// Every language the interface can be shown in, English first.
    pub fn languages(&self) -> Vec<Language> {
        let include_sublime = self.global_settings().bool("import_sublime_packages", true);
        let mut languages = vec![Language { code: ENGLISH.into(), name: "English".into() }];
        for language in self.gateways.resources.languages(include_sublime) {
            if !languages.iter().any(|l| l.code == language.code) {
                languages.push(language);
            }
        }
        languages
    }

    /// Saves the chosen language (a code, or [`AUTOMATIC`]) and switches the interface to it.
    pub fn select_language(&mut self, code: &str) {
        self.persist_setting("language", Value::String(code.to_string()));
        let current = self.state.translations.language().to_string();
        let name = self.languages().into_iter().find(|l| l.code == current).map(|l| l.name).unwrap_or(current);
        self.notify(self.tr_args("Language: {name}", &[("name", &name)]));
    }

    /// A document's name as tabs and messages show it: [`Document::name`], with "untitled" translated.
    pub fn document_name(&self, doc: &Document) -> String {
        let name = doc.name();
        if name == UNTITLED && doc.path.is_none() {
            self.tr("untitled").to_string()
        } else {
            name
        }
    }

    /// The interface language in use.
    pub fn translations(&self) -> Rc<Translations> {
        self.state.translations.clone()
    }

    /// `msgid` in the interface language.
    pub fn tr<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.state.translations.tr(msgid)
    }

    /// `msgid` in the interface language, with its `{placeholders}` filled.
    pub fn tr_args(&self, msgid: &str, args: &[(&str, &dyn Display)]) -> String {
        self.state.translations.tr_args(msgid, args)
    }

    /// The singular or plural message for `count`, in the interface language.
    pub fn tr_n(&self, singular: &str, plural: &str, count: usize, args: &[(&str, &dyn Display)]) -> String {
        self.state.translations.tr_n(singular, plural, count as u64, args)
    }
}
