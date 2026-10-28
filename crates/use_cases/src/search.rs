//! Use cases: the quick panel searches (Command Palette, Goto Anything, color scheme and language pickers).

use std::path::PathBuf;

use entities::command::Command;
use entities::document::DocumentId;
use entities::localization::Translations;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::editor::Editor;
use crate::localization::AUTOMATIC;

const MAX_RESULTS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    /// Command Palette: palette entries plus "Set Syntax: …" for every syntax.
    Commands,
    /// Goto Anything: files (`name`), lines (`:42`) and symbols (`@name`).
    Goto,
    ColorSchemes,
    /// The interface language picker.
    Languages,
    /// The items of a plugin's `window.show_quick_panel`.
    PluginItems,
}

/// What choosing a result does.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateTarget {
    Command(Command),
    OpenFile(PathBuf),
    Document(DocumentId),
    Line(usize),
    ColorScheme(String),
    /// A language code, or `auto`.
    Language(String),
    /// The index of an item in a plugin's quick panel (the controller answers the plugin).
    PluginItem(usize),
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub caption: String,
    /// Second line (e.g. the file's path).
    pub detail: String,
    /// Right-aligned hint (e.g. the key binding or the line number).
    pub hint: String,
    pub target: CandidateTarget,
}

impl Candidate {
    fn new(caption: impl Into<String>, target: CandidateTarget) -> Self {
        Self {
            caption: caption.into(),
            detail: String::new(),
            hint: String::new(),
            target,
        }
    }
}

impl Editor {
    /// Results for a query, best match first.
    pub fn search(&self, kind: SearchKind, query: &str) -> Vec<Candidate> {
        let matcher = SkimMatcherV2::default();
        let score = |text: &str, query: &str| {
            if query.is_empty() {
                Some(0)
            } else {
                matcher.fuzzy_match(text, query)
            }
        };
        let mut scored: Vec<(i64, Candidate)> = Vec::new();

        match kind {
            SearchKind::Commands => {
                // Captions are shown translated but also match in English, the language of
                // Sublime's documentation and of prefilled queries such as `Set Syntax: `.
                let score_both = |caption: &str, english: &str| score(caption, query).max(score(english, query));
                for entry in &self.state.palette {
                    let caption = self.tr(&entry.caption);
                    if let Some(s) = score_both(caption, &entry.caption) {
                        let mut c = Candidate::new(caption, CandidateTarget::Command(entry.command.clone()));
                        c.hint = self.state.keymap.binding_for(&entry.command).unwrap_or_default();
                        scored.push((s, c));
                    }
                }
                for syntax in self.gateways.syntaxes.syntax_names() {
                    let caption = self.tr_args("Set Syntax: {syntax}", &[("syntax", &syntax)]);
                    if let Some(s) = score_both(&caption, &format!("Set Syntax: {syntax}")) {
                        let command = Command::with_args("set_file_type", serde_json::json!({ "syntax": syntax }));
                        scored.push((s, Candidate::new(caption, CandidateTarget::Command(command))));
                    }
                }
            }
            SearchKind::ColorSchemes => {
                for name in self.gateways.syntaxes.color_scheme_names() {
                    if let Some(s) = score(&name, query) {
                        scored.push((s, Candidate::new(&name, CandidateTarget::ColorScheme(name.clone()))));
                    }
                }
            }
            SearchKind::Languages => {
                let automatic = self.tr("Automatic (system language)").to_string();
                let languages = std::iter::once((automatic, AUTOMATIC.to_string()))
                    .chain(self.languages().into_iter().map(|l| (l.name, l.code)));
                for (name, code) in languages {
                    if let Some(s) = score(&name, query).max(score(&code, query)) {
                        let mut c = Candidate::new(name, CandidateTarget::Language(code.clone()));
                        c.hint = if code == AUTOMATIC { String::new() } else { code };
                        // An empty query keeps the order: automatic, English, then the others.
                        scored.push((if query.is_empty() { -(scored.len() as i64) } else { s }, c));
                    }
                }
            }
            SearchKind::PluginItems => {
                let items = self.plugin_quick_panel().map(|p| p.items.as_slice()).unwrap_or_default();
                for (index, item) in items.iter().enumerate() {
                    if let Some(s) = score(&item.caption, query).max(score(&item.detail, query)) {
                        let mut c = Candidate::new(&item.caption, CandidateTarget::PluginItem(index));
                        c.detail = item.detail.clone();
                        c.hint = item.hint.clone();
                        scored.push((s, c));
                    }
                }
            }
            SearchKind::Goto => {
                if let Some(line) = query.strip_prefix(':') {
                    return vec![goto_line_candidate(line, &self.state.translations)];
                }
                if let Some(symbol_query) = query.strip_prefix('@') {
                    return self.symbol_candidates(symbol_query.trim(), &matcher);
                }
                self.file_candidates(query, &matcher, &mut scored);
            }
        }
        scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        scored.truncate(MAX_RESULTS);
        scored.into_iter().map(|(_, c)| c).collect()
    }

    fn symbol_candidates(&self, query: &str, matcher: &SkimMatcherV2) -> Vec<Candidate> {
        let Some(doc) = self.active_document() else {
            return Vec::new();
        };
        let symbols = self.gateways.syntaxes.symbols(doc.id, &doc.text, &doc.syntax);
        let mut scored: Vec<(i64, Candidate)> = symbols
            .into_iter()
            .filter_map(|symbol| {
                let s = if query.is_empty() { Some(0) } else { matcher.fuzzy_match(&symbol.name, query) }?;
                let mut c = Candidate::new(&symbol.name, CandidateTarget::Line(symbol.line));
                c.hint = (symbol.line + 1).to_string();
                Some((s, c))
            })
            .collect();
        if !query.is_empty() {
            // An empty query keeps file order.
            scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        }
        scored.into_iter().map(|(_, c)| c).collect()
    }

    fn file_candidates(&self, query: &str, matcher: &SkimMatcherV2, scored: &mut Vec<(i64, Candidate)>) {
        let open_paths: Vec<&PathBuf> = self.state.documents.iter().filter_map(|d| d.path.as_ref()).collect();
        for doc in &self.state.documents {
            let name = self.document_name(doc);
            let s = if query.is_empty() { Some(0) } else { matcher.fuzzy_match(&name, query) };
            if let Some(s) = s {
                let mut c = Candidate::new(name, CandidateTarget::Document(doc.id));
                c.detail = doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                c.hint = self.tr("open").to_string();
                // Open files rank slightly above other files.
                scored.push((s + 10, c));
            }
        }
        for path in self.gateways.file_index.files() {
            if open_paths.contains(&path) {
                continue;
            }
            let relative = self
                .state
                .folders
                .iter()
                .find_map(|f| path.strip_prefix(f).ok())
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let s = if query.is_empty() {
                Some(0)
            } else {
                // A match on the file name counts more than one on the folder path.
                let by_name = matcher.fuzzy_match(&name, query).map(|s| s + 20);
                by_name.max(matcher.fuzzy_match(&relative, query))
            };
            if let Some(s) = s {
                let mut c = Candidate::new(name, CandidateTarget::OpenFile(path.clone()));
                c.detail = relative;
                scored.push((s, c));
            }
        }
    }

    /// Carries out a chosen result. A palette command is returned for the caller to run.
    pub fn accept_candidate(&mut self, target: CandidateTarget) -> Option<Command> {
        match target {
            CandidateTarget::Command(command) => return Some(command),
            CandidateTarget::OpenFile(path) => {
                self.open_file(&path);
            }
            CandidateTarget::Document(id) => self.activate_document(id),
            CandidateTarget::Line(line) => self.go_to_line(line),
            CandidateTarget::ColorScheme(name) => self.apply_color_scheme(&name),
            CandidateTarget::Language(code) => self.select_language(&code),
            // Answered by the controller, which can run the commands the plugin's callback runs.
            CandidateTarget::PluginItem(_) => {}
        }
        None
    }

    /// Live preview while moving through results or typing (color schemes, `:line`).
    pub fn preview_candidate(&mut self, target: &CandidateTarget) {
        match target {
            CandidateTarget::ColorScheme(name) => self.preview_color_scheme(name),
            CandidateTarget::Line(line) => self.go_to_line(*line),
            _ => {}
        }
    }
}

fn goto_line_candidate(line: &str, translations: &Translations) -> Candidate {
    match line.trim().parse::<usize>() {
        Ok(n) => Candidate::new(translations.tr_args("Go to line {line}", &[("line", &n)]), CandidateTarget::Line(n.saturating_sub(1))),
        Err(_) => Candidate::new(translations.tr("Type a line number"), CandidateTarget::Line(0)),
    }
}
