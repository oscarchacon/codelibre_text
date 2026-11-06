//! Use cases: auto-complete. The list opens as the user types a word (`auto_complete`,
//! `auto_complete_selector`) or a trigger character (`auto_complete_triggers`), or with the
//! `auto_complete` command (Ctrl+Space); it filters itself as the word grows and goes when the
//! caret leaves the word, another view becomes active, Esc, or a completion is committed.
//!
//! Its completions come from the plugins (`on_query_completions`, also `CompletionList`s filled
//! later with `set_completions`), the packages' `.sublime-completions` files and snippets with a
//! `tabTrigger` whose scope matches, and the words of the text. Tab with nothing open expands a
//! trigger typed in full (`tab_completion`, Sublime's `insert_best_completion`).

use entities::command::Command;
use entities::completion::{parse_command_completion, words_near, Completion, CompletionFormat, CompletionSet, CompletionSource, CompletionKind};
use entities::document::DocumentId;
use entities::selection::Selection;
use entities::selector::matches_selector;
use entities::text::is_word_char;
use entities::text_commands as tc;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::editor::Editor;

/// Sublime's `INHIBIT_WORD_COMPLETIONS`, `INHIBIT_EXPLICIT_COMPLETIONS`, `DYNAMIC_COMPLETIONS`,
/// `INHIBIT_REORDER`.
pub const INHIBIT_WORD_COMPLETIONS: u64 = 8;
pub const INHIBIT_EXPLICIT_COMPLETIONS: u64 = 16;
pub const DYNAMIC_COMPLETIONS: u64 = 32;
pub const INHIBIT_REORDER: u64 = 128;
/// Sublime's default `auto_complete_selector`.
const DEFAULT_SELECTOR: &str = "meta.tag, source - comment - string.quoted.double.block - string.quoted.single.block - string.unquoted.heredoc";
const DEFAULT_SIZE_LIMIT: usize = 4_194_304;
/// At most this many words of the text are offered.
const MAX_WORDS: usize = 1000;

/// A completion the list shows: which one, and which chars of its trigger the typed text matched.
#[derive(Debug, Clone, PartialEq)]
pub struct ShownCompletion {
    pub completion: Completion,
    pub matched: Vec<usize>,
}

/// The auto-complete list of a view.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoComplete {
    pub view: DocumentId,
    /// Where the typed word starts: committing replaces from here to the caret.
    pub start: usize,
    /// What was typed (from `start` to the caret) when the list was last filtered.
    pub prefix: String,
    pub shown: Vec<ShownCompletion>,
    pub selected: usize,
    items: Vec<Completion>,
    /// Opened by the command or a trigger character: it stays with nothing typed.
    sticky: bool,
    flags: u64,
    /// Plugins' `CompletionList`s still to be filled (`set_completions`).
    pending: Vec<u64>,
    /// The triggers committed before, newest first (`auto_complete_use_history`): they go first.
    history: Vec<String>,
}

/// A list waiting for `auto_complete_delay` to open.
#[derive(Debug, Clone)]
pub(crate) struct DueAutoComplete {
    view: DocumentId,
    due: Instant,
    sticky: bool,
}

/// The completion Tab inserted with no list open (`insert_best_completion`), which Tab again
/// replaces with the next one (`replace_completion_with_next_completion`) while nothing else
/// changed the text.
#[derive(Debug, Clone)]
pub(crate) struct LastCompletion {
    view: DocumentId,
    /// Where the completion is in the text (chars), and the change count right after it.
    start: usize,
    end: usize,
    change_count: u64,
    candidates: Vec<Completion>,
    index: usize,
}

/// At most this many committed triggers are remembered (`auto_complete_use_history`).
const HISTORY_LIMIT: usize = 100;

/// What the sources offered for one prefix.
#[derive(Default)]
struct Gathered {
    items: Vec<Completion>,
    flags: u64,
    pending: Vec<u64>,
}

/// The `auto_complete` command's arguments.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoCompleteOptions {
    /// Only the plugins' completions (`api_completions_only`).
    pub api_only: bool,
    /// Only snippets (`snippets_only`).
    pub snippets_only: bool,
    /// One completion is inserted at once (`commit_single`).
    pub commit_single: bool,
}

impl Editor {
    pub fn auto_complete(&self) -> Option<&AutoComplete> {
        self.state.auto_complete.as_ref().filter(|a| !a.shown.is_empty())
    }

    pub fn auto_complete_visible(&self) -> bool {
        self.auto_complete().is_some()
    }

    /// The `auto_complete` command (Ctrl+Space): opens the list at the active view's caret.
    pub fn show_auto_complete(&mut self, options: AutoCompleteOptions) {
        let Some(view) = self.active_document().map(|d| d.id) else { return };
        self.open_auto_complete(view, true, options);
        if options.commit_single && self.state.auto_complete.as_ref().is_some_and(|a| a.shown.len() == 1 && a.pending.is_empty()) {
            self.commit_completion(None);
        }
    }

    /// The text widget typed `typed` in `view`: a word character opens the list where
    /// `auto_complete_selector` matches, a character of `auto_complete_triggers` where its selector does.
    pub fn typed_in_view(&mut self, view: DocumentId, typed: &str) {
        if self.state.auto_complete.is_some() || self.active_document().map(|d| d.id) != Some(view) {
            return;
        }
        let settings = self.settings_of(view);
        let Some(doc) = self.any_document(view) else { return };
        if !settings.bool("auto_complete", true)
            || doc.text.len() > settings.usize("auto_complete_size_limit", DEFAULT_SIZE_LIMIT)
            || !doc.selection().is_empty()
        {
            return;
        }
        let Some(last) = typed.chars().last() else { return };
        let caret = doc.selection().head;
        // `rhs_empty` triggers: only with nothing but blanks after the caret on its line.
        let rest_of_line: String = doc.text.chars().skip(caret).take_while(|c| *c != '\n').collect();
        let scope = self.scope_name(view, caret.saturating_sub(1)).unwrap_or_default();
        let triggers = settings.get("auto_complete_triggers").and_then(Value::as_array).cloned().unwrap_or_default();
        let triggered = triggers.iter().any(|trigger| {
            let characters = trigger.get("characters").and_then(Value::as_str).unwrap_or("");
            let selector = trigger.get("selector").and_then(Value::as_str).unwrap_or("");
            let rhs_empty = trigger.get("rhs_empty").and_then(Value::as_bool).unwrap_or(false);
            characters.contains(last)
                && (selector.is_empty() || matches_selector(&scope, selector))
                && (!rhs_empty || rest_of_line.trim().is_empty())
        });
        let sticky = if triggered {
            true
        } else if is_word_char(last) {
            let selector = settings.str("auto_complete_selector").unwrap_or(DEFAULT_SELECTOR).to_string();
            if !(selector.is_empty() || matches_selector(&scope, &selector)) {
                return;
            }
            false
        } else {
            return;
        };
        // `auto_complete_delay`: the list opens once typing stops for that long.
        let delay = settings.usize("auto_complete_delay", 0);
        if delay == 0 {
            self.open_auto_complete(view, sticky, AutoCompleteOptions::default());
        } else {
            let due = Instant::now() + Duration::from_millis(delay as u64);
            self.state.auto_complete_due = Some(DueAutoComplete { view, due, sticky });
        }
    }

    /// A list waits for `auto_complete_delay`: the window must draw again soon.
    pub fn auto_complete_due(&self) -> Option<Instant> {
        self.state.auto_complete_due.as_ref().map(|d| d.due)
    }

    /// Opens the list that waited for `auto_complete_delay` once its time came, if the caret is
    /// still after a word (or a trigger) in that view.
    fn open_due_auto_complete(&mut self) {
        let Some(due) = &self.state.auto_complete_due else { return };
        if Instant::now() < due.due {
            return;
        }
        let Some(due) = self.state.auto_complete_due.take() else { return };
        let still_typing = self.active_document().filter(|d| d.id == due.view).is_some_and(|d| {
            let caret = d.selection().head;
            d.selection().is_empty() && (due.sticky || caret > word_start(&d.text, caret))
        });
        if still_typing {
            self.open_auto_complete(due.view, due.sticky, AutoCompleteOptions::default());
        }
    }

    fn open_auto_complete(&mut self, view: DocumentId, sticky: bool, options: AutoCompleteOptions) {
        let Some(doc) = self.any_document(view) else { return };
        let caret = doc.selection().head;
        let start = word_start(&doc.text, caret);
        let prefix: String = doc.text.chars().skip(start).take(caret - start).collect();
        let gathered = self.gather_completions(view, start, caret, &prefix, options);
        let mut list = AutoComplete {
            view,
            start,
            prefix,
            shown: Vec::new(),
            selected: 0,
            items: gathered.items,
            sticky,
            flags: gathered.flags,
            pending: gathered.pending,
            history: self.completion_history(view),
        };
        list.filter();
        self.state.auto_complete = (!list.shown.is_empty() || !list.pending.is_empty()).then_some(list);
    }

    /// The triggers committed before, when `auto_complete_use_history` is on.
    fn completion_history(&self, view: DocumentId) -> Vec<String> {
        if self.settings_of(view).bool("auto_complete_use_history", false) {
            self.state.completion_history.clone()
        } else {
            Vec::new()
        }
    }

    /// Every frame: the list follows the typed word, and goes when the caret leaves it or another
    /// view becomes active. Plugins that asked for it (`DYNAMIC_COMPLETIONS`) are asked again.
    pub(crate) fn update_auto_complete(&mut self) {
        self.open_due_auto_complete();
        let Some(list) = &self.state.auto_complete else { return };
        let view = list.view;
        let prefix = match self.active_document().filter(|d| d.id == view) {
            Some(doc) if doc.selection().is_empty() && doc.selection().head >= list.start => {
                let caret = doc.selection().head;
                let prefix: String = doc.text.chars().skip(list.start).take(caret - list.start).collect();
                prefix.chars().all(is_word_char).then_some(prefix)
            }
            _ => None,
        };
        let Some(prefix) = prefix.filter(|p| !p.is_empty() || list.sticky) else {
            self.hide_auto_complete();
            return;
        };
        if prefix == list.prefix {
            return;
        }
        let Some(mut list) = self.state.auto_complete.take() else { return };
        if list.flags & DYNAMIC_COMPLETIONS != 0 {
            let caret = list.start + prefix.chars().count();
            let plugins = self.plugin_completions(view, &prefix, caret);
            list.items.retain(|c| c.source != CompletionSource::Plugin);
            list.items.splice(0..0, plugins.items);
            list.pending = plugins.pending;
        }
        list.prefix = prefix;
        list.filter();
        if !list.shown.is_empty() || !list.pending.is_empty() {
            self.state.auto_complete = Some(list);
        }
    }

    pub fn hide_auto_complete(&mut self) {
        self.state.auto_complete = None;
        self.state.auto_complete_due = None;
    }

    /// Up / Down / Page Up / Page Down in the list. Up and Down wrap around at its ends with
    /// `auto_complete_cycle`; otherwise they stop there.
    pub fn move_auto_complete_selection(&mut self, delta: isize) {
        let cycle = self.view_settings().bool("auto_complete_cycle", false);
        if let Some(list) = self.state.auto_complete.as_mut().filter(|l| !l.shown.is_empty()) {
            let count = list.shown.len() as isize;
            let next = list.selected as isize + delta;
            list.selected = if delta.abs() == 1 && cycle { next.rem_euclid(count) } else { next.clamp(0, count - 1) } as usize;
        }
    }

    /// `commit_completion` (Enter, Tab, a click): inserts the selected completion (or the one at
    /// `index`) in place of the typed word. A command completion's command runs at the next
    /// [`Editor::poll_plugins`].
    pub fn commit_completion(&mut self, index: Option<usize>) {
        let Some(list) = self.state.auto_complete.take() else { return };
        if let Some(shown) = list.shown.get(index.unwrap_or(list.selected)) {
            self.insert_completion(list.view, list.start, &shown.completion);
        }
    }

    /// Sublime's `insert_best_completion` (Tab): with the list open, commits its selection;
    /// otherwise inserts the completion whose trigger is the word before the caret (`exact`), or
    /// the best one. False when there was none.
    pub fn insert_best_completion(&mut self, exact: bool) -> bool {
        if self.auto_complete_visible() {
            self.commit_completion(None);
            return true;
        }
        let Some(doc) = self.active_document() else { return false };
        let (view, caret) = (doc.id, doc.selection().head);
        if !doc.selection().is_empty() {
            return false;
        }
        let start = word_start(&doc.text, caret);
        if start == caret {
            return false;
        }
        let prefix: String = doc.text.chars().skip(start).take(caret - start).collect();
        let options = AutoCompleteOptions { api_only: false, snippets_only: false, commit_single: false };
        let gathered = self.gather_completions(view, start, caret, &prefix, options);
        let candidates: Vec<Completion> = if exact {
            gathered.items.into_iter().filter(|c| c.source != CompletionSource::Word && c.trigger == prefix).take(1).collect()
        } else {
            let mut list = AutoComplete {
                view,
                start,
                prefix,
                shown: Vec::new(),
                selected: 0,
                items: gathered.items,
                sticky: false,
                flags: gathered.flags,
                pending: Vec::new(),
                history: self.completion_history(view),
            };
            list.filter();
            list.shown.into_iter().map(|s| s.completion).collect()
        };
        let Some(best) = candidates.first().cloned() else { return false };
        self.insert_completion(view, start, &best);
        // Tab again tries the next candidate (`replace_completion_with_next_completion`).
        if let Some(doc) = self.any_document(view) {
            let end = doc.selection().head;
            self.state.last_completion = Some(LastCompletion { view, start, end, change_count: doc.change_count(), candidates, index: 0 });
        }
        true
    }

    /// The `last_command` context with `insert_best_completion`: Tab inserted a completion and
    /// nothing changed the text or moved the caret since.
    pub fn last_command_was_completion(&self) -> bool {
        let Some(last) = &self.state.last_completion else { return false };
        self.active_document()
            .is_some_and(|d| d.id == last.view && d.change_count() == last.change_count && d.selection() == Selection::caret(last.end))
    }

    /// `replace_completion_with_next_completion` (Tab after Tab): the completion just inserted
    /// gives way to the next candidate (the first again after the last).
    pub fn replace_completion_with_next(&mut self) {
        if !self.last_command_was_completion() {
            return;
        }
        let Some(mut last) = self.state.last_completion.take() else { return };
        if last.candidates.len() < 2 {
            self.state.last_completion = Some(last);
            return;
        }
        last.index = (last.index + 1) % last.candidates.len();
        let next = last.candidates[last.index].clone();
        let Some(doc) = self.any_document_mut(last.view) else { return };
        doc.set_selection(Selection::new(last.start, last.end));
        // The completion replaces the selection, as if it had been typed over it.
        doc.edit(|text, selection| Some(tc::replace_selection(text, selection, "")));
        let start = last.start;
        self.insert_completion(last.view, start, &next);
        if let Some(doc) = self.any_document(last.view) {
            last.end = doc.selection().head;
            last.change_count = doc.change_count();
            self.state.last_completion = Some(last);
        }
    }

    /// Tab with nothing selected: a snippet or completion trigger typed in full is expanded
    /// (`tab_completion`). False when Tab should do what it always does.
    pub fn tab_completion(&mut self) -> bool {
        if self.auto_complete_visible() {
            self.commit_completion(None);
            return true;
        }
        self.view_settings().bool("tab_completion", true) && self.insert_best_completion(true)
    }

    /// A plugin's `CompletionList.set_completions`: the list it belongs to shows them, if it is still open.
    pub(crate) fn fill_completion_list(&mut self, id: u64, items: &Value, flags: u64) {
        let Some(list) = self.state.auto_complete.as_mut().filter(|l| l.pending.contains(&id)) else { return };
        list.pending.retain(|p| *p != id);
        list.flags |= flags;
        let new: Vec<Completion> = parse_items(items);
        let at = list.items.iter().position(|c| c.source != CompletionSource::Plugin).unwrap_or(list.items.len());
        list.items.splice(at..at, new);
        list.filter();
        if list.shown.is_empty() && list.pending.is_empty() {
            self.hide_auto_complete();
        }
    }

    /// `view.extract_completions(prefix, point)`: the words of the text that start with `prefix`
    /// (not the one at `point`, the caret by default).
    pub(crate) fn extract_completions(&self, view: DocumentId, prefix: &str, point: Option<usize>) -> Vec<String> {
        let Some(doc) = self.any_document(view) else { return Vec::new() };
        let caret = point.unwrap_or(doc.selection().head).min(doc.char_count());
        let lower = prefix.to_lowercase();
        let words = words_near(&doc.text, caret, &word_around(&doc.text, caret), MAX_WORDS);
        words.into_iter().filter(|w| w.to_lowercase().starts_with(&lower)).collect()
    }

    /// Inserts a completion in place of the word typed from `start` to the caret. With several
    /// carets, each one gets it in place of the same word typed before it, as in Sublime; a
    /// snippet's fields are visited at the primary caret only when it is the only one.
    fn insert_completion(&mut self, view: DocumentId, start: usize, completion: &Completion) {
        self.remember_completion(view, &completion.trigger);
        let Some(doc) = self.any_document_mut(view) else { return };
        let caret = doc.selection().head;
        if caret < start {
            return;
        }
        let from = if completion.keep_prefix { caret } else { start };
        let replaced = Selection::new(from, caret);
        if !doc.extra_selections().is_empty() && completion.format != CompletionFormat::Command {
            let prefix: String = doc.text.chars().skip(start).take(caret - start).collect();
            let (keep, contents, snippet) = (completion.keep_prefix, completion.contents.clone(), completion.format == CompletionFormat::Snippet);
            doc.edit_all(|text, selections| {
                tc::complete_at_each(text, selections, &prefix, keep, |text, at| {
                    if snippet { tc::insert_snippet(text, at, &contents) } else { tc::replace_selection(text, at, &contents) }
                })
            });
            return;
        }
        match completion.format {
            CompletionFormat::Text => {
                doc.edit(|text, _| Some(tc::replace_selection(text, replaced, &completion.contents)));
            }
            CompletionFormat::Snippet => self.insert_snippet_over(view, Some(replaced), &completion.contents),
            CompletionFormat::Command => {
                doc.edit(|text, _| Some(tc::replace_selection(text, replaced, "")));
                let (name, args) = parse_command_completion(&completion.contents);
                if !name.is_empty() {
                    self.queue_plugin_command(Command::with_args(&name, args));
                }
            }
        }
    }

    /// `auto_complete_use_history`: a committed trigger comes first next time.
    fn remember_completion(&mut self, view: DocumentId, trigger: &str) {
        if !self.settings_of(view).bool("auto_complete_use_history", false) || trigger.is_empty() {
            return;
        }
        let history = &mut self.state.completion_history;
        history.retain(|t| t != trigger);
        history.insert(0, trigger.to_string());
        history.truncate(HISTORY_LIMIT);
    }

    /// The completions for `prefix` (typed from `start` to the caret): the plugins' first, then
    /// the packages' and the text's words, unless the plugins or the options leave them out.
    fn gather_completions(&mut self, view: DocumentId, start: usize, caret: usize, prefix: &str, options: AutoCompleteOptions) -> Gathered {
        let mut gathered = if options.snippets_only { Gathered::default() } else { self.plugin_completions(view, prefix, caret) };
        if options.api_only {
            return gathered;
        }
        if gathered.flags & INHIBIT_EXPLICIT_COMPLETIONS == 0 {
            let scope = self.scope_name(view, start.max(caret.saturating_sub(1))).unwrap_or_default();
            let snippet = CompletionKind::snippet();
            for set in self.completion_sets() {
                if set.scope.is_empty() || matches_selector(&scope, &set.scope) {
                    let wanted = set.completions.iter().filter(|c| !options.snippets_only || c.kind == snippet);
                    gathered.items.extend(wanted.cloned());
                }
            }
        }
        if gathered.flags & INHIBIT_WORD_COMPLETIONS == 0 && !options.snippets_only {
            if let Some(doc) = self.any_document(view) {
                let typed = word_around(&doc.text, caret);
                let mut words = words_near(&doc.text, caret, &typed, MAX_WORDS);
                // Then the words of the other open views, most recently used first.
                let others = self.state.recently_used.iter().filter(|id| **id != view).filter_map(|id| self.any_document(*id));
                for other in others {
                    if words.len() >= MAX_WORDS {
                        break;
                    }
                    let limit = MAX_WORDS - words.len();
                    words.extend(words_near(&other.text, 0, &typed, limit).into_iter().filter(|w| !words.contains(w)).collect::<Vec<_>>());
                }
                for word in words {
                    if !gathered.items.iter().any(|c| c.trigger == word) {
                        gathered.items.push(Completion::word(&word));
                    }
                }
            }
        }
        gathered
    }

    /// `on_query_completions(view, prefix, locations)` of every listener, merged by the host:
    /// `{"items": […], "flags": n, "pending": [list ids]}`.
    fn plugin_completions(&mut self, view: DocumentId, prefix: &str, caret: usize) -> Gathered {
        let Some(answer) = self.plugin_query("on_query_completions", Some(view), json!([prefix, [caret]])) else {
            return Gathered::default();
        };
        Gathered {
            items: answer.get("items").map(parse_items).unwrap_or_default(),
            flags: answer.get("flags").and_then(Value::as_u64).unwrap_or(0),
            pending: answer.get("pending").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_u64).collect()).unwrap_or_default(),
        }
    }

    /// The packages' `.sublime-completions` and snippets with a `tabTrigger`, read once after
    /// each load of the packages.
    fn completion_sets(&mut self) -> &[CompletionSet] {
        if self.state.completion_sets.is_none() {
            let include = self.global_settings().bool("import_sublime_packages", true);
            let resources = &self.gateways.resources;
            let mut sets = Vec::new();
            for name in resources.find_resources("*.sublime-completions", include) {
                if let Ok(value) = resources.load_json_resource(&name, include) {
                    sets.push(CompletionSet::from_completions_file(&value));
                }
            }
            for name in resources.find_resources("*.sublime-snippet", include) {
                let Ok(bytes) = resources.load_resource(&name, include) else { continue };
                if let Some(set) = CompletionSet::from_snippet(&String::from_utf8_lossy(&bytes)) {
                    sets.push(set);
                }
            }
            self.state.completion_sets = Some(sets);
        }
        self.state.completion_sets.as_deref().unwrap_or_default()
    }
}

impl AutoComplete {
    /// The items that match the prefix (its chars in order, the first at the start of a word of
    /// the trigger), best first: a trigger that starts with the prefix, then the matcher's score,
    /// then the plugins' and packages' before the words. `INHIBIT_REORDER` keeps the plugins' order.
    fn filter(&mut self) {
        let matcher = SkimMatcherV2::default().ignore_case();
        let previous = self.shown.get(self.selected).map(|s| s.completion.trigger.clone());
        let lower = self.prefix.to_lowercase();
        let mut scored: Vec<(bool, i64, usize, ShownCompletion)> = Vec::new();
        for (index, completion) in self.items.iter().enumerate() {
            let (score, matched) = if self.prefix.is_empty() {
                (0, Vec::new())
            } else {
                match matcher.fuzzy_indices(&completion.trigger, &self.prefix) {
                    Some((score, matched)) if starts_a_word(&completion.trigger, matched[0]) => (score, matched),
                    _ => continue,
                }
            };
            let starts = completion.trigger.to_lowercase().starts_with(&lower);
            scored.push((starts, score, index, ShownCompletion { completion: completion.clone(), matched }));
        }
        // With `auto_complete_use_history`, what was committed before comes first, newest first.
        let remembered = |shown: &ShownCompletion| self.history.iter().position(|t| *t == shown.completion.trigger).unwrap_or(usize::MAX);
        if self.flags & INHIBIT_REORDER != 0 {
            scored.sort_by_key(|(_, _, index, shown)| (shown.completion.source, *index));
        } else {
            scored.sort_by_key(|(starts, score, index, shown)| (remembered(shown), !starts, -score, shown.completion.source, *index));
        }
        self.shown = scored.into_iter().map(|(_, _, _, shown)| shown).collect();
        // Only the word typed in full: nothing to complete.
        if !self.sticky && self.shown.len() == 1 && self.shown[0].completion.trigger == self.prefix && self.shown[0].completion.contents == self.prefix {
            self.shown.clear();
        }
        self.selected = previous.and_then(|t| self.shown.iter().position(|s| s.completion.trigger == t)).unwrap_or(0);
    }
}

/// The completions a plugin sent (already in Sublime's object form, see `sublime_plugin.py`).
fn parse_items(items: &Value) -> Vec<Completion> {
    let items = items.as_array().map(Vec::as_slice).unwrap_or_default();
    items.iter().filter_map(|c| Completion::from_json(c, CompletionSource::Plugin)).collect()
}

/// The whole word the char `caret` is in or next to (what is being typed: not a completion).
fn word_around(text: &str, caret: usize) -> String {
    let word = entities::text::word_at(text, caret);
    text.chars().skip(word.start()).take(word.end() - word.start()).collect()
}

/// The char where the word that ends at `caret` starts.
fn word_start(text: &str, caret: usize) -> usize {
    let before: Vec<char> = text.chars().take(caret).collect();
    let length = before.iter().rev().take_while(|c| is_word_char(**c)).count();
    caret - length
}

/// Whether the char at `index` of `trigger` begins a word: the first char, one after a non-word
/// char, or an upper-case letter after a lower-case one (`getLength`).
fn starts_a_word(trigger: &str, index: usize) -> bool {
    let chars: Vec<char> = trigger.chars().collect();
    match (index.checked_sub(1).map(|i| chars[i]), chars.get(index)) {
        (None, _) => true,
        (Some(before), Some(&at)) => !is_word_char(before) || (before.is_lowercase() && at.is_uppercase()) || (!before.is_ascii_digit() && at.is_ascii_digit()),
        _ => false,
    }
}
