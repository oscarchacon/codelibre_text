//! Use cases: Find in Files (`show_panel {"panel": "find_in_files"}`, Ctrl+Shift+F).
//!
//! The panel's "Where" field says what to search ([`entities::find_in_files::parse_where`]); the
//! search runs in the background ([`FileSearchGateway`](crate::gateways::FileSearchGateway)) and
//! its results go to the output panel `find_results` as Sublime writes them, with Sublime's
//! `result_file_regex` / `result_line_regex`: `next_result` (F4) and a double click go to them.
//! With "Use Buffer" on they go to a "Find Results" tab instead (each search added at its end,
//! the matches outlined). Replacing searches first, then asks, then replaces in each file's tab
//! (left unsaved, as Sublime does).

use std::path::{Path, PathBuf};

use entities::document::{DocumentId, RegionSet};
use entities::find::{self, FindOptions};
use entities::find_in_files::{
    add_where_entry, format_file, parse_where, shorten_line, where_snippet_text, FileHits,
    PathFilter, RESULT_FILE_REGEX, RESULT_LINE_REGEX,
};
use entities::selection::Selection;
use entities::text::{byte_to_char, char_to_byte};
use serde_json::{json, Map, Value};

use crate::editor::Editor;
use crate::gateways::{
    platform_name, FileSearchEvent, FileSearchRequest, MessageAnswer, MessageButtons,
    MessageDialog, MessageLevel,
};

/// The output panel Find in Files writes to (Sublime's name).
pub const FIND_RESULTS_PANEL: &str = "find_results";
/// The region set outlining the matches in the tab.
pub const FIND_RESULTS_MATCHES: &str = "find_results_matches";
/// Sublime's `sublime.DRAW_NO_FILL`.
const DRAW_NO_FILL: u64 = 32;

#[derive(Default)]
pub struct FindInFilesState {
    /// The panel's "Where" field.
    where_text: String,
    /// "Use Buffer": the results go to a tab instead of the panel.
    use_buffer: bool,
    /// The "Find Results" tab, while it is open.
    tab: Option<DocumentId>,
    running: Option<Search>,
    /// The options of the last search: going to one of its results selects the match.
    last_options: Option<FindOptions>,
}

struct Search {
    /// Where the results go: the tab's view, or the panel (`None`).
    tab: Option<DocumentId>,
    options: FindOptions,
    /// Replace with this once the search ends.
    replace: Option<String>,
    files_with_matches: Vec<PathBuf>,
    matches: usize,
    context_characters: usize,
    suppress_errors: bool,
}

impl Editor {
    /// The find panel's "Where" field.
    pub fn find_where(&self) -> &str {
        &self.state.find_in_files.where_text
    }

    pub fn set_find_where(&mut self, text: String) {
        self.state.find_in_files.where_text = text;
    }

    /// `add_where_snippet {"snippet"}`: adds an entry (`*.txt`, `<open files>`…) to "Where".
    pub fn add_where_snippet(&mut self, snippet: &str) {
        let entry = where_snippet_text(snippet);
        let text = add_where_entry(self.find_where(), &entry);
        self.set_find_where(text);
    }

    /// `add_directory`: asks for a folder and adds it to "Where".
    pub fn prompt_add_where_folder(&mut self) {
        let start = self.state.folders.first().cloned();
        if let Some(folder) = self.gateways.dialogs.pick_folder(start.as_deref()) {
            self.add_where_snippet(&folder.display().to_string());
        }
    }

    /// `clear_location`.
    pub fn clear_where(&mut self) {
        self.set_find_where(String::new());
    }

    /// "Use Buffer": Find in Files writes to a "Find Results" tab instead of the panel.
    pub fn find_use_buffer(&self) -> bool {
        self.state.find_in_files.use_buffer
    }

    pub fn set_find_use_buffer(&mut self, on: bool) {
        self.state.find_in_files.use_buffer = on;
    }

    /// The "Find Results" tab, while it is open.
    pub fn find_results_tab(&self) -> Option<DocumentId> {
        let id = self.state.find_in_files.tab?;
        self.state
            .documents
            .iter()
            .any(|d| d.id == id)
            .then_some(id)
    }

    /// Whether `id` is a view Find in Files writes to (its panel's or its tab's): going to one of
    /// its results selects the match.
    pub(crate) fn is_find_results_view(&self, id: DocumentId) -> bool {
        self.find_output_panel(FIND_RESULTS_PANEL) == Some(id)
            || self.find_results_tab() == Some(id)
    }

    pub fn is_finding_in_files(&self) -> bool {
        self.state.find_in_files.running.is_some()
    }

    /// `find_all` / `replace_all` in the Find in Files panel: searches what "Where" lists, into
    /// the `find_results` panel (shown). With `replace`, the replacement follows once the search
    /// ends and the user agrees. False when no search started (empty query, invalid regex, nothing
    /// to search).
    pub fn find_in_files(&mut self, replace: bool) -> bool {
        let options = self.state.find.clone();
        match options.pattern() {
            Ok(Some(_)) => self.state.find_error = None,
            Ok(None) => return false,
            Err(error) => {
                self.state.find_error = Some(error);
                return false;
            }
        }
        let place = parse_where(self.find_where());
        let roots = self.search_roots(&place);
        if roots.is_empty() {
            self.notify(self.tr("No open folders or files to search").to_string());
            return false;
        }
        let settings = self.global_settings();
        let filter = PathFilter {
            include: place.include,
            exclude: place.exclude,
            folder_excludes: settings.strings("folder_exclude_patterns"),
            file_excludes: settings.strings("file_exclude_patterns"),
            binary: settings.strings("binary_file_patterns"),
        };
        let open_texts = self
            .state
            .documents
            .iter()
            .filter_map(|d| Some((d.path.clone()?, d.text.clone())))
            .collect();
        let request = FileSearchRequest {
            roots,
            filter,
            options: options.clone(),
            context_lines: settings.usize("find_in_files_context_lines", 2),
            max_file_size: settings.usize("find_in_files_max_file_size", 104_857_600) as u64,
            open_texts,
        };
        let tab = if self.find_use_buffer() {
            Some(self.open_find_results_tab())
        } else {
            self.open_find_results_panel();
            None
        };
        self.state.find_in_files.last_options = Some(options.clone());
        self.state.find_in_files.running = Some(Search {
            tab,
            options,
            replace: replace.then(|| self.state.replacement.clone()),
            files_with_matches: Vec::new(),
            matches: 0,
            context_characters: settings.usize("find_in_files_context_characters", 300),
            suppress_errors: settings.bool("find_in_files_suppress_errors", false),
        });
        self.gateways.file_search.start(request);
        true
    }

    /// `cancel_find`: stops the search running.
    pub fn cancel_find_in_files(&mut self) {
        if let Some(search) = self.state.find_in_files.running.take() {
            self.gateways.file_search.cancel();
            let text = self.tr("Find in Files cancelled").to_string();
            self.write_results(search.tab, &format!("{text}\n"));
            self.notify(text);
        }
    }

    /// Every frame: writes what the search found to `find_results`, and its end.
    pub fn poll_find_in_files(&mut self) {
        if self.state.find_in_files.running.is_none() {
            return;
        }
        for event in self.gateways.file_search.poll() {
            match event {
                FileSearchEvent::Listed(files) => {
                    let header = self.search_header(files);
                    self.append_find_results(&format!("{header}\n\n"));
                }
                FileSearchEvent::Found(hits) => self.write_file_hits(&hits),
                FileSearchEvent::Failed(path, error) => {
                    let suppress = self
                        .state
                        .find_in_files
                        .running
                        .as_ref()
                        .is_some_and(|s| s.suppress_errors);
                    if !suppress {
                        let path = self.results_path(&path);
                        self.append_find_results(&format!("{path}:\n    ERROR: {error}\n\n"));
                    }
                }
                FileSearchEvent::Done => {
                    self.finish_find_in_files();
                    return;
                }
            }
        }
    }

    /// After going to a result of Find Results (the caret at its line's start): selects the first
    /// match on that line, as Sublime does.
    pub(crate) fn select_find_result_match(&mut self) {
        let Some(Ok(Some(pattern))) = self
            .state
            .find_in_files
            .last_options
            .as_ref()
            .map(FindOptions::pattern)
        else {
            return;
        };
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let start = char_to_byte(&doc.text, doc.selection().head);
        let end = doc.text[start..]
            .find('\n')
            .map_or(doc.text.len(), |i| start + i);
        let found = pattern
            .find_iter(&doc.text[start..])
            .map(|m| start + m.start()..start + m.end())
            .find(|m| !m.is_empty())
            .filter(|m| m.start <= end);
        if let Some(m) = found {
            let selection = Selection::new(
                byte_to_char(&doc.text, m.start),
                byte_to_char(&doc.text, m.end),
            );
            doc.set_selection(selection);
        }
    }

    fn search_roots(&self, place: &entities::find_in_files::Where) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut add = |path: PathBuf| {
            if !roots.contains(&path) {
                roots.push(path);
            }
        };
        if place.open_folders {
            self.state.folders.iter().cloned().for_each(&mut add);
        }
        if place.open_files {
            self.state
                .documents
                .iter()
                .filter_map(|d| d.path.clone())
                .for_each(&mut add);
        }
        if place.current_file {
            if let Some(path) = self.active_document().and_then(|d| d.path.clone()) {
                add(path);
            }
        }
        for entry in &place.paths {
            add(self.where_path(entry));
        }
        roots
    }

    /// A folder or file of "Where": `~` is the home folder, a relative path is under the first
    /// open folder.
    fn where_path(&self, entry: &str) -> PathBuf {
        let home = self.gateways.shell.home_folder();
        let path = match (entry.strip_prefix('~'), home) {
            (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
                home.join(rest.trim_start_matches(['/', '\\']))
            }
            _ => PathBuf::from(entry),
        };
        match self.state.folders.first() {
            Some(folder) if path.is_relative() => folder.join(path),
            _ => path,
        }
    }

    /// Empties `find_results` (or makes it), sets it up for results and shows it.
    fn open_find_results_panel(&mut self) {
        let id = self.create_output_panel(FIND_RESULTS_PANEL, false);
        self.state
            .plugins
            .view_settings
            .insert(id, results_settings());
        if let Some(doc) = self.any_document_mut(id) {
            doc.read_only = true;
        }
        self.show_output_panel(FIND_RESULTS_PANEL);
        self.remember_results_panel(FIND_RESULTS_PANEL);
    }

    /// "Use Buffer": the "Find Results" tab (made the first time, a blank line added to it after
    /// an earlier search), shown; its view id.
    fn open_find_results_tab(&mut self) -> DocumentId {
        let id = match self.find_results_tab() {
            Some(id) => {
                let ends_open = self
                    .any_document(id)
                    .is_some_and(|d| !d.text.is_empty() && !d.text.ends_with("\n\n"));
                if ends_open {
                    let _ = self.append_text(id, "\n", true);
                }
                self.activate_document(id);
                id
            }
            None => {
                let name = self.tr("Find Results").to_string();
                self.open_scratch(&name, "", "Plain Text", false);
                let id = self.active_document().map(|d| d.id).expect("just opened");
                self.state.find_in_files.tab = Some(id);
                id
            }
        };
        self.state
            .plugins
            .view_settings
            .insert(id, results_settings());
        if let Some(doc) = self.any_document_mut(id) {
            doc.scratch = true;
        }
        self.remember_results_tab(id);
        id
    }

    fn append_find_results(&mut self, text: &str) {
        let tab = self
            .state
            .find_in_files
            .running
            .as_ref()
            .and_then(|s| s.tab);
        self.write_results(tab, text);
    }

    /// Adds `text` to the tab `tab`, or to the panel. In the tab, the matches of the lines with
    /// one (`  12: `) are outlined.
    fn write_results(&mut self, tab: Option<DocumentId>, text: &str) {
        let Some(id) = tab.or_else(|| self.find_output_panel(FIND_RESULTS_PANEL)) else {
            return;
        };
        let Some(start) = self.any_document(id).map(|d| d.char_count()) else {
            return;
        };
        if self.append_text(id, text, true).is_err() || tab.is_none() {
            return;
        }
        let Some(Ok(Some(pattern))) = self
            .state
            .find_in_files
            .running
            .as_ref()
            .map(|s| s.options.pattern())
        else {
            return;
        };
        let mut found = Vec::new();
        let mut line_start = 0;
        for line in text.split_inclusive('\n') {
            if let Some(prefix) = match_line_prefix(line) {
                let content = line[prefix..].trim_end_matches(['\n', '\r']);
                for m in pattern.find_iter(content).filter(|m| !m.is_empty()) {
                    let at = |byte: usize| start + byte_to_char(text, line_start + prefix + byte);
                    found.push((at(m.start()), at(m.end())));
                }
            }
            line_start += line.len();
        }
        if found.is_empty() {
            return;
        }
        if let Some(doc) = self.any_document_mut(id) {
            let mut regions = doc.regions(FIND_RESULTS_MATCHES);
            regions.extend(found);
            doc.add_regions(RegionSet {
                key: FIND_RESULTS_MATCHES.to_string(),
                regions,
                flags: DRAW_NO_FILL,
                ..RegionSet::default()
            });
        }
    }

    /// `Searching 3 files for "foo" (regex, case sensitive, whole word)`.
    fn search_header(&self, files: usize) -> String {
        let Some(search) = &self.state.find_in_files.running else {
            return String::new();
        };
        let options = &search.options;
        let query = options.query.clone();
        let mut header = self.tr_n(
            "Searching {count} file for \"{query}\"",
            "Searching {count} files for \"{query}\"",
            files,
            &[("query", &query)],
        );
        let flags: Vec<&str> = [
            options.regex.then(|| self.tr("regex")),
            options.case_sensitive.then(|| self.tr("case sensitive")),
            options.whole_word.then(|| self.tr("whole word")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !flags.is_empty() {
            header.push_str(&format!(" ({})", flags.join(", ")));
        }
        header
    }

    fn write_file_hits(&mut self, hits: &FileHits) {
        let Some(limit) = self
            .state
            .find_in_files
            .running
            .as_ref()
            .map(|s| s.context_characters)
        else {
            return;
        };
        let skipped = |n: usize| {
            self.tr_n(
                "<skipped {count} character>",
                "<skipped {count} characters>",
                n,
                &[],
            )
        };
        let lines: Vec<(usize, bool, String)> = hits
            .lines
            .iter()
            .map(|line| {
                (
                    line.row,
                    !line.matches.is_empty(),
                    shorten_line(line, limit, &skipped),
                )
            })
            .collect();
        let text = format_file(&self.results_path(&hits.path), &lines);
        self.append_find_results(&text);
        if let Some(search) = &mut self.state.find_in_files.running {
            search.matches += hits.matches;
            search.files_with_matches.push(hits.path.clone());
        }
    }

    /// The path as Find Results writes it: the home folder as `~` (not on Windows).
    fn results_path(&self, path: &Path) -> String {
        if platform_name() != "Windows" {
            if let Some(rest) = self
                .gateways
                .shell
                .home_folder()
                .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
            {
                return Path::new("~").join(rest).display().to_string();
            }
        }
        path.display().to_string()
    }

    fn finish_find_in_files(&mut self) {
        let Some(search) = self.state.find_in_files.running.take() else {
            return;
        };
        let files = search.files_with_matches.len();
        let summary = match (search.matches, files) {
            (0, _) => self.tr("0 matches").to_string(),
            (matches, 1) => self.tr_n(
                "{count} match in 1 file",
                "{count} matches in 1 file",
                matches,
                &[],
            ),
            (matches, files) => self.tr_n(
                "{count} match across {files} files",
                "{count} matches across {files} files",
                matches,
                &[("files", &files)],
            ),
        };
        self.write_results(search.tab, &format!("{summary}\n"));
        self.notify(summary);
        if let Some(replacement) = search.replace {
            if search.matches > 0 && self.confirm_replace_in_files(search.matches, files) {
                self.replace_in_files(&search.options, &replacement, &search.files_with_matches);
            }
        }
    }

    fn confirm_replace_in_files(&self, matches: usize, files: usize) -> bool {
        let text = self.tr_n(
            "Replace {count} occurrence across {files} files?",
            "Replace {count} occurrences across {files} files?",
            matches,
            &[("files", &files)],
        );
        let dialog = MessageDialog {
            level: MessageLevel::Warning,
            title: self.tr("Replace in Files").to_string(),
            text,
            buttons: MessageButtons::OkCancel {
                ok: self.tr("Replace").to_string(),
                cancel: self.tr("Cancel").to_string(),
            },
        };
        self.gateways.dialogs.show_message(&dialog) == MessageAnswer::Ok
    }

    /// Opens each file and replaces its matches in the tab, which stays unsaved.
    fn replace_in_files(&mut self, options: &FindOptions, replacement: &str, files: &[PathBuf]) {
        let Ok(Some(pattern)) = options.pattern() else {
            return;
        };
        let mut count = 0;
        for path in files {
            if !self.open_file(path) {
                continue;
            }
            if let Some(doc) = self.active_document_mut() {
                doc.edit(|text, sel| {
                    count += find::replace_all(&pattern, text, replacement, options);
                    Some(sel.clamped(text.chars().count()))
                });
            }
        }
        self.notify(self.tr_n(
            "Replaced {count} occurrence",
            "Replaced {count} occurrences",
            count,
            &[],
        ));
    }
}

/// The view settings of a view Find in Files writes to: its results are places in files.
fn results_settings() -> Map<String, Value> {
    Map::from_iter([
        ("result_file_regex".to_string(), json!(RESULT_FILE_REGEX)),
        ("result_line_regex".to_string(), json!(RESULT_LINE_REGEX)),
        ("result_base_dir".to_string(), json!("")),
        ("word_wrap".to_string(), json!(false)),
    ])
}

/// In a line of Find Results, the length of `  12: ` when the line is a match.
fn match_line_prefix(line: &str) -> Option<usize> {
    let digits = line.trim_start_matches(' ');
    let spaces = line.len() - digits.len();
    let number = digits.len()
        - digits
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    (spaces > 0 && number > 0 && digits[number..].starts_with(": ")).then_some(spaces + number + 2)
}
