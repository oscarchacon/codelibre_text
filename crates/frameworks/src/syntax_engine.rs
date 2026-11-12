//! syntect (a Rust port of Sublime Text's highlighting engine): syntax definitions, color schemes
//! and per-document highlighting caches.
//!
//! One instance is shared (`Rc<RefCell<…>>`) by the [`SyntaxGateway`](use_cases::gateways::SyntaxGateway)
//! implementation, which the use cases call, and by the editor view, which paints with it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;

use egui::text::{LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, FontId, Stroke};
use serde_json::{Map, Value};
use entities::document::DocumentId;
use entities::symbol::Symbol;
use interface_adapters::gateways::color_scheme_file::{self, ColorSchemeDefinition, Rgba};
use interface_adapters::gateways::{builtin_resources, package_files};
use syntect::highlighting::{
    Color, FontStyle, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, Style, StyleModifier,
    Theme, ThemeItem, ThemeSet, ThemeSettings,
};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet};
use crate::sublime_syntax::SyntaxFiles;
use use_cases::gateways::{ContextFrame, PackagePaths, ScopeSpan, SyntaxInfo};

pub type SharedSyntaxEngine = Rc<RefCell<SyntaxEngine>>;

const DEFAULT_SCHEME: &str = "Monokai";
/// Files larger than this are shown without highlighting.
const MAX_HIGHLIGHT_BYTES: usize = 4 * 1024 * 1024;

pub struct SyntaxEngine {
    paths: PackagePaths,
    syntaxes: SyntaxSet,
    /// Color schemes by file stem (e.g. `Monokai`, `Solarized (dark)`).
    schemes: BTreeMap<String, Theme>,
    caches: HashMap<DocumentId, HighlightCache>,
    /// Scheme of the last paint, so symbol lookups reuse the same caches.
    current_scheme: String,
    /// `scope_color` answers by (scheme, scope).
    scope_colors: HashMap<(String, String), Option<Color32>>,
    /// The resource each package syntax came from, by syntax name.
    syntax_paths: HashMap<String, String>,
    /// The names of each syntax's contexts, by syntect's context index (see [`context_names`]);
    /// worked out the first time a backtrace needs them when the packages were not loaded.
    context_names: Option<Vec<Vec<String>>>,
    /// Highlighting that panicked (see [`guard`]), for the editor's error log, taken once a frame.
    failures: Vec<String>,
}

/// syntect's `ContextId`s are (index of the syntax in the set, index of the context among the
/// syntax's contexts sorted by name), and its parser state does not say more than those ids (in
/// its `Debug` text): the names, by index, for a backtrace.
fn context_names(builder: &syntect::parsing::SyntaxSetBuilder) -> Vec<Vec<String>> {
    builder
        .syntaxes()
        .iter()
        .map(|syntax| {
            let mut names: Vec<String> = syntax.contexts.keys().cloned().collect();
            names.sort_unstable();
            names
        })
        .collect()
}

/// The `(syntax index, context index)` of each level of a parser state, outermost first, read
/// from its `Debug` text (`StateLevel { context: ContextId { syntax_index: 1, context_index: 5 }…`).
fn context_ids(state: &ParseState) -> Vec<(usize, usize)> {
    let text = format!("{state:?}");
    let number_after = |s: &str, key: &str| -> Option<usize> {
        let at = s.find(key)? + key.len();
        s[at..].trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    };
    text.split("context: ContextId {")
        .skip(1)
        .filter_map(|piece| Some((number_after(piece, "syntax_index:")?, number_after(piece, "context_index:")?)))
        .collect()
}

/// Sublime's `region.<color>ish` scopes, for color schemes without rules for them.
fn region_color(scope: &str) -> Option<Color32> {
    let name = scope.strip_prefix("region.")?;
    Some(match name.split('.').next()? {
        "redish" => Color32::from_rgb(235, 96, 106),
        "orangish" => Color32::from_rgb(242, 151, 78),
        "yellowish" => Color32::from_rgb(230, 200, 90),
        "greenish" => Color32::from_rgb(130, 200, 120),
        "cyanish" => Color32::from_rgb(95, 200, 200),
        "bluish" => Color32::from_rgb(100, 150, 230),
        "purplish" => Color32::from_rgb(180, 130, 220),
        "pinkish" => Color32::from_rgb(230, 130, 190),
        _ => return None,
    })
}

/// Runs `work`, and turns a panic into its message. syntect panics when a regex it cannot compile
/// turns up while highlighting; that must not close the editor, the text is then shown plain.
pub fn guard<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| message.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string())
    })
}

impl SyntaxEngine {
    /// Starts with syntect's built-in syntaxes and our built-in color schemes;
    /// `reload` adds the ones from packages.
    pub fn new(paths: PackagePaths) -> Self {
        let mut engine = Self {
            paths,
            syntaxes: SyntaxSet::load_defaults_newlines(),
            schemes: BTreeMap::new(),
            caches: HashMap::new(),
            current_scheme: DEFAULT_SCHEME.to_string(),
            scope_colors: HashMap::new(),
            syntax_paths: HashMap::new(),
            context_names: None,
            failures: Vec::new(),
        };
        engine.load_builtin_schemes(&mut Vec::new());
        engine
    }

    fn load_builtin_schemes(&mut self, errors: &mut Vec<String>) {
        self.schemes = ThemeSet::load_defaults().themes.into_iter().collect();
        for (name, source) in builtin_resources::COLOR_SCHEMES {
            match color_scheme_file::parse(source, name) {
                Ok(definition) => {
                    self.schemes.insert(name.to_string(), to_syntect_theme(&definition));
                }
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
    }

    pub fn reload(&mut self, include_sublime: bool, errors: &mut Vec<String>) {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        self.syntax_paths.clear();
        // Sublime's shipped syntaxes are only parents of the packages' (`extends`): syntect has
        // its own for those languages, which need no branches.
        let shipped: Vec<PathBuf> = self.paths.archive_folders.iter().filter(|f| f.shipped).map(|f| f.unpacked.clone()).collect();
        let mut files = SyntaxFiles::new();
        let mut to_load = Vec::new();
        for path in package_files::find_resources(&self.paths, include_sublime, |n| n.ends_with(".sublime-syntax")) {
            let resource = package_files::resource_name(&self.paths, include_sublime, &path).unwrap_or_else(|| path.display().to_string());
            match std::fs::read_to_string(&path) {
                Ok(text) => files.add(&resource, text),
                Err(e) => {
                    errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
            }
            if !shipped.iter().any(|root| path.starts_with(root)) {
                to_load.push((resource, path));
            }
        }
        for (resource, path) in to_load {
            let fallback_name = path.file_stem().and_then(|s| s.to_str());
            let definition = files.for_syntect(&resource).and_then(|rewritten| {
                let text = match rewritten {
                    Some(text) => text,
                    None => std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
                };
                SyntaxDefinition::load_from_str(&text, true, fallback_name).map_err(|e| e.to_string())
            });
            match definition {
                Ok(definition) => {
                    self.syntax_paths.insert(definition.name.clone(), resource);
                    builder.add(definition);
                }
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        // syntect numbers the contexts of each syntax in the order of their names.
        self.context_names = Some(context_names(&builder));
        self.syntaxes = builder.build();

        self.load_builtin_schemes(errors);
        for path in package_files::find_resources(&self.paths, include_sublime, |n| {
            n.ends_with(".tmTheme") || n.ends_with(".sublime-color-scheme")
        }) {
            let name = file_stem(&path);
            let json = !path.extension().is_some_and(|e| e == "tmTheme");
            let theme = if json {
                std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|source| color_scheme_file::parse(&source, &name))
                    .map(|definition| to_syntect_theme(&definition))
            } else {
                ThemeSet::get_theme(&path).map_err(|e| e.to_string())
            };
            match theme {
                // Sublime names a color scheme by its file name, and a second file with that name
                // patches the first (MarkdownEditing's and LSP's Monokai, a user's Monokai): its
                // globals override and its rules are appended. A `.tmTheme` replaces instead.
                Ok(theme) => match self.schemes.get_mut(&name) {
                    Some(base) if json => merge_theme(base, theme),
                    _ => {
                        self.schemes.insert(name, theme);
                    }
                },
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        self.caches.clear();
        self.scope_colors.clear();
    }

    /// The color scheme with that name (or the default one).
    pub fn scheme(&self, name: &str) -> &Theme {
        find_scheme(&self.schemes, name)
    }

    pub fn scheme_names(&self) -> Vec<String> {
        self.schemes.keys().cloned().collect()
    }

    fn syntax(&self, name: &str) -> &SyntaxReference {
        find_syntax(&self.syntaxes, name)
    }

    pub fn detect(&self, path: &Path, first_line: &str) -> String {
        let by_name = path.file_name().and_then(|n| n.to_str()).and_then(|name| {
            self.syntaxes.find_syntax_by_extension(name).or_else(|| {
                path.extension()
                    .and_then(|e| e.to_str())
                    .and_then(|e| self.syntaxes.find_syntax_by_extension(e))
            })
        });
        by_name
            .or_else(|| self.syntaxes.find_syntax_by_first_line(first_line))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
            .name
            .clone()
    }

    pub fn resolve(&self, name_or_path: &str) -> String {
        let name = Path::new(name_or_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| name_or_path.to_string());
        self.syntax(&name).name.clone()
    }

    pub fn syntax_path(&self, syntax: &str) -> String {
        let name = &self.syntax(syntax).name;
        match self.syntax_paths.get(name) {
            Some(path) => path.clone(),
            None => builtin_syntax_path(name),
        }
    }

    pub fn base_scope(&self, syntax: &str) -> String {
        find_syntax(&self.syntaxes, syntax).scope.to_string()
    }

    pub fn syntax_names(&self) -> Vec<String> {
        self.syntaxes.syntaxes().iter().filter(|s| !s.hidden).map(|s| s.name.clone()).collect()
    }

    pub fn take_failures(&mut self) -> Vec<String> {
        std::mem::take(&mut self.failures)
    }

    pub fn forget(&mut self, document: DocumentId) {
        self.caches.remove(&document);
    }

    /// The color a color scheme gives a scope (a plugin region's `region.redish`, `invalid`…):
    /// its rule's foreground, or for the `region.<color>ish` scopes without a rule, that color.
    pub fn scope_color(&mut self, scheme: &str, scope: &str) -> Option<Color32> {
        let key = (scheme.to_string(), scope.to_string());
        if let Some(color) = self.scope_colors.get(&key) {
            return *color;
        }
        let first = scope.split_whitespace().next().unwrap_or_default();
        let from_scheme = ScopeStack::from_str(first).ok().and_then(|stack| {
            Highlighter::new(find_scheme(&self.schemes, scheme)).style_mod_for_stack(stack.as_slice()).foreground
        });
        let color = from_scheme.map(to_color32).or_else(|| region_color(first));
        self.scope_colors.insert(key, color);
        color
    }

    /// A color scheme's global colors, as plugins see them (`view.style()`).
    pub fn scheme_style(&self, scheme: &str) -> Map<String, Value> {
        let settings = &find_scheme(&self.schemes, scheme).settings;
        let foreground = settings.foreground.unwrap_or(Color { r: 0xf8, g: 0xf8, b: 0xf2, a: 0xff });
        let background = settings.background.unwrap_or(Color { r: 0x27, g: 0x28, b: 0x22, a: 0xff });
        let colors = [
            ("foreground", Some(foreground)),
            ("background", Some(background)),
            ("caret", settings.caret.or(Some(foreground))),
            ("line_highlight", settings.line_highlight),
            ("misspelling", settings.misspelling),
            ("minimap_border", settings.minimap_border),
            ("accent", settings.accent),
            ("highlight", settings.highlight),
            ("find_highlight", settings.find_highlight),
            ("find_highlight_foreground", settings.find_highlight_foreground),
            ("gutter", settings.gutter),
            ("gutter_foreground", settings.gutter_foreground),
            ("selection", settings.selection),
            ("selection_foreground", settings.selection_foreground),
            ("selection_border", settings.selection_border),
            ("inactive_selection", settings.inactive_selection),
            ("inactive_selection_foreground", settings.inactive_selection_foreground),
            ("guide", settings.guide),
            ("active_guide", settings.active_guide),
            ("stack_guide", settings.stack_guide),
            ("shadow", settings.shadow),
            ("brackets_foreground", settings.brackets_foreground),
            ("brackets_background", settings.brackets_background),
            ("bracket_contents_foreground", settings.bracket_contents_foreground),
            ("tags_foreground", settings.tags_foreground),
        ];
        colors.into_iter().filter_map(|(name, color)| Some((name.to_string(), Value::String(hex(color?))))).collect()
    }

    /// The palette Sublime derives from a color scheme (`ui_info()["color_scheme"]["palette"]`):
    /// its global colors, then `redish`, `orangish`… — the closest color the scheme defines for
    /// each hue (themes and plugins use them for terminal colors, diagnostics…).
    pub fn scheme_palette(&self, scheme: &str) -> Map<String, Value> {
        let theme = find_scheme(&self.schemes, scheme);
        let settings = &theme.settings;
        let mut candidates: Vec<Color> = [
            settings.foreground,
            settings.background,
            settings.caret,
            settings.accent,
            settings.selection,
            settings.line_highlight,
            settings.gutter_foreground,
        ]
        .into_iter()
        .flatten()
        .collect();
        candidates.extend(theme.scopes.iter().filter_map(|item| item.style.foreground));
        let mut palette = self.scheme_style(scheme);
        for (name, hue) in [
            ("redish", 0.0),
            ("orangish", 30.0),
            ("yellowish", 60.0),
            ("greenish", 120.0),
            ("cyanish", 180.0),
            ("bluish", 220.0),
            ("purplish", 280.0),
            ("pinkish", 320.0),
        ] {
            if let Some(color) = closest_hue(&candidates, hue) {
                palette.insert(name.to_string(), Value::String(hex(color)));
            }
        }
        palette
    }

    /// What a color scheme gives a stack of scopes (`view.style_for_scope()`).
    pub fn scope_style(&self, scheme: &str, scope: &str) -> Map<String, Value> {
        let theme = find_scheme(&self.schemes, scheme);
        let modifier = ScopeStack::from_str(scope)
            .map(|stack| Highlighter::new(theme).style_mod_for_stack(stack.as_slice()))
            .unwrap_or_default();
        let foreground = modifier
            .foreground
            .or(theme.settings.foreground)
            .unwrap_or(Color { r: 0xf8, g: 0xf8, b: 0xf2, a: 0xff });
        let mut style = Map::new();
        style.insert("foreground".into(), Value::String(hex(foreground)));
        if let Some(background) = modifier.background {
            style.insert("background".into(), Value::String(hex(background)));
        }
        let font_style = modifier.font_style.unwrap_or_else(FontStyle::empty);
        for (name, flag) in [("bold", FontStyle::BOLD), ("italic", FontStyle::ITALIC), ("underline", FontStyle::UNDERLINE)] {
            if font_style.contains(flag) {
                style.insert(name.into(), Value::Bool(true));
            }
        }
        style
    }

    pub fn symbols(&mut self, document: DocumentId, text: &str, syntax: &str) -> Vec<Symbol> {
        let syntax = self.syntax(syntax).clone();
        let theme = self.scheme(&self.current_scheme).clone();
        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, &syntax, &theme, &mut self.failures);
        cache.symbols()
    }

    /// The scopes of one line, kept by the highlighting cache (which parses only what changed
    /// since the last paint). Text too big to highlight, or plain text, is the syntax's base scope.
    pub fn line_scopes(&mut self, document: DocumentId, text: &str, syntax: &str, line: usize) -> Vec<ScopeSpan> {
        let Some(line_text) = text.split_inclusive('\n').nth(line) else {
            return Vec::new();
        };
        let syntax = self.syntax(syntax).clone();
        if text.len() > MAX_HIGHLIGHT_BYTES || syntax.name == "Plain Text" {
            return vec![ScopeSpan { range: 0..line_text.len(), scope: syntax.scope.to_string() }];
        }
        let theme = self.scheme(&self.current_scheme).clone();
        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, &syntax, &theme, &mut self.failures);
        cache.lines.get(line).map(|l| l.scopes.clone()).unwrap_or_default()
    }

    /// The scopes of every line (see [`Self::line_scopes`]).
    pub fn document_scopes(&mut self, document: DocumentId, text: &str, syntax: &str) -> Vec<Vec<ScopeSpan>> {
        self.lines_scopes(document, text, syntax, 0..usize::MAX)
    }

    /// The scopes of the lines `lines` (the range stops at the last line): the cache is brought up
    /// to date once and only those lines are copied.
    pub fn lines_scopes(&mut self, document: DocumentId, text: &str, syntax: &str, lines: Range<usize>) -> Vec<Vec<ScopeSpan>> {
        let syntax = self.syntax(syntax).clone();
        let take = lines.end.saturating_sub(lines.start);
        if text.len() > MAX_HIGHLIGHT_BYTES || syntax.name == "Plain Text" {
            let scope = syntax.scope.to_string();
            return text
                .split_inclusive('\n')
                .skip(lines.start)
                .take(take)
                .map(|line| vec![ScopeSpan { range: 0..line.len(), scope: scope.clone() }])
                .collect();
        }
        let theme = self.scheme(&self.current_scheme).clone();
        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, &syntax, &theme, &mut self.failures);
        cache.lines.iter().skip(lines.start).take(take).map(|l| l.scopes.clone()).collect()
    }

    /// The parser's contexts at byte `column` of `line`: the line's start state (from the cache)
    /// parsed up to that column, its context ids named (see [`context_names`]).
    pub fn context_backtrace(&mut self, document: DocumentId, text: &str, syntax: &str, line: usize, column: usize) -> Vec<ContextFrame> {
        let syntax = self.syntax(syntax).clone();
        if text.len() > MAX_HIGHLIGHT_BYTES || syntax.name == "Plain Text" {
            return Vec::new();
        }
        let theme = self.scheme(&self.current_scheme).clone();
        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, &syntax, &theme, &mut self.failures);
        let Some(cached) = cache.lines.get(line) else { return Vec::new() };
        let mut state = cached.state_before.0.clone();
        let mut end = column.min(cached.text.len());
        while !cached.text.is_char_boundary(end) {
            end -= 1;
        }
        if end > 0 {
            match guard(|| state.parse_line(&cached.text[..end], &self.syntaxes).is_err()) {
                Ok(false) => {}
                Ok(true) => return Vec::new(),
                Err(message) => {
                    self.failures.push(format!("{}: backtrace of line {} failed: {message}", syntax.name, line + 1));
                    return Vec::new();
                }
            }
        }
        let names = self.context_names.get_or_insert_with(|| context_names(&self.syntaxes.clone().into_builder()));
        let syntaxes = self.syntaxes.syntaxes();
        context_ids(&state)
            .into_iter()
            .filter_map(|(syntax_index, context_index)| {
                let name = names.get(syntax_index)?.get(context_index)?;
                let syntax = syntaxes.get(syntax_index)?;
                // syntect wraps `main` in its own `__start` / `__main` contexts.
                let name = match name.as_str() {
                    "__start" => return None,
                    "__main" => "main",
                    anonymous if anonymous.starts_with("#anon") => "anonymous context",
                    other => other,
                };
                Some(ContextFrame {
                    context_name: name.to_string(),
                    source_file: self.syntax_paths.get(&syntax.name).cloned().unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Every syntax, hidden ones too, with the resource it came from.
    pub fn syntax_infos(&self) -> Vec<SyntaxInfo> {
        self.syntaxes
            .syntaxes()
            .iter()
            .map(|s| SyntaxInfo {
                path: self.syntax_paths.get(&s.name).cloned().unwrap_or_else(|| s.name.clone()),
                name: s.name.clone(),
                hidden: s.hidden,
                scope: s.scope.to_string(),
                file_extensions: s.file_extensions.clone(),
            })
            .collect()
    }

    /// The text of a document as colored egui text, re-highlighting only what changed.
    pub fn layout_job(&mut self, document: DocumentId, text: &str, syntax: &str, scheme: &str, font: FontId) -> LayoutJob {
        if self.current_scheme != scheme {
            self.current_scheme = scheme.to_string();
        }
        let syntax = find_syntax(&self.syntaxes, syntax);
        let theme = find_scheme(&self.schemes, scheme);
        let default_color = theme.settings.foreground.map(to_color32).unwrap_or(Color32::LIGHT_GRAY);
        let plain = |font: FontId| LayoutJob {
            text: text.to_string(),
            sections: vec![LayoutSection {
                leading_space: 0.0,
                byte_range: 0..text.len(),
                format: TextFormat::simple(font, default_color),
            }],
            ..Default::default()
        };
        if text.len() > MAX_HIGHLIGHT_BYTES || syntax.name == "Plain Text" {
            return plain(font);
        }

        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, syntax, theme, &mut self.failures);
        let mut job = LayoutJob { text: text.to_string(), ..Default::default() };
        let mut offset = 0;
        for line in &cache.lines {
            for (range, style) in &line.spans {
                job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: offset + range.start..offset + range.end,
                    format: TextFormat {
                        font_id: font.clone(),
                        color: to_color32(style.foreground),
                        italics: style.font_style.contains(FontStyle::ITALIC),
                        underline: if style.font_style.contains(FontStyle::UNDERLINE) {
                            Stroke::new(1.0_f32, to_color32(style.foreground))
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    },
                });
            }
            offset += line.text.len();
        }
        if job.sections.is_empty() {
            return plain(font);
        }
        job
    }
}

/// Highlighting of one document, cached per line with the parser state at the start of each line.
/// An edit re-highlights from the first changed line until the state matches the cache again.
#[derive(Default)]
struct HighlightCache {
    /// (syntax, color scheme) the cache was built with.
    key: (String, String),
    lines: Vec<CachedLine>,
}

/// A highlighted line: its styled spans, its Goto Symbol name and its scopes.
type Highlighted = (Vec<(Range<usize>, Style)>, Option<String>, Vec<ScopeSpan>);

#[derive(Clone)]
struct CachedLine {
    text: String,
    state_before: (ParseState, HighlightState),
    spans: Vec<(Range<usize>, Style)>,
    symbol: Option<String>,
    /// The line's scopes (bytes of the line), for plugins and the `selector` context.
    scopes: Vec<ScopeSpan>,
}

impl HighlightCache {
    fn symbols(&self) -> Vec<Symbol> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(line, l)| l.symbol.as_ref().map(|name| Symbol { line, name: name.clone() }))
            .collect()
    }

    fn update(&mut self, text: &str, syntaxes: &SyntaxSet, syntax: &SyntaxReference, theme: &Theme, failures: &mut Vec<String>) {
        let key = (syntax.name.clone(), theme.name.clone().unwrap_or_default());
        if key != self.key {
            self.lines.clear();
            self.key = key;
        }

        let new_lines: Vec<&str> = text.split_inclusive('\n').collect();
        let unchanged = self.lines.iter().zip(&new_lines).take_while(|(old, new)| old.text == **new).count();
        if unchanged == new_lines.len() && unchanged == self.lines.len() {
            return;
        }

        let highlighter = Highlighter::new(theme);
        let old = std::mem::take(&mut self.lines);
        let mut lines: Vec<CachedLine> = old[..unchanged].to_vec();
        // Once syntect panics, its state is not trusted: the rest of the text is shown plain.
        let mut failed = false;
        let mut state = match (old.get(unchanged), unchanged.checked_sub(1).and_then(|i| old.get(i))) {
            (Some(line), _) => line.state_before.clone(),
            (None, Some(previous)) => {
                // Appending after the last cached line: continue from its end state.
                let mut s = previous.state_before.clone();
                match guard(|| highlight_line(&previous.text, &mut s, syntaxes, &highlighter)) {
                    Ok(_) => s,
                    Err(message) => {
                        failed = true;
                        failures.push(format!("{}: highlighting stopped at line {}: {message}", syntax.name, unchanged));
                        s
                    }
                }
            }
            (None, None) => (
                ParseState::new(syntax),
                HighlightState::new(&highlighter, ScopeStack::new()),
            ),
        };

        // Lines after the edit keep their old index shifted by the number of added/removed lines.
        let shift = new_lines.len() as isize - old.len() as isize;
        for (i, &line_text) in new_lines.iter().enumerate().skip(unchanged) {
            let old_index = i as isize - shift;
            if !failed && old_index >= unchanged as isize {
                if let Some(old_line) = old.get(old_index as usize) {
                    if old_line.text == line_text && old_line.state_before == state {
                        lines.extend_from_slice(&old[old_index as usize..]);
                        break;
                    }
                }
            }
            let state_before = state.clone();
            let (spans, symbol, scopes) = if failed {
                plain_line(line_text, syntax)
            } else {
                match guard(|| highlight_line(line_text, &mut state, syntaxes, &highlighter)) {
                    Ok(highlighted) => highlighted,
                    Err(message) => {
                        failed = true;
                        failures.push(format!("{}: highlighting stopped at line {}: {message}", syntax.name, i + 1));
                        plain_line(line_text, syntax)
                    }
                }
            };
            lines.push(CachedLine {
                text: line_text.to_string(),
                state_before,
                spans,
                symbol,
                scopes,
            });
        }
        self.lines = lines;
    }
}

/// A line shown as the syntax's base scope, with no colors: what a line that cannot be highlighted gets.
fn plain_line(line: &str, syntax: &SyntaxReference) -> Highlighted {
    (Vec::new(), None, vec![ScopeSpan { range: 0..line.len(), scope: syntax.scope.to_string() }])
}

fn highlight_line(
    line: &str,
    state: &mut (ParseState, HighlightState),
    syntaxes: &SyntaxSet,
    highlighter: &Highlighter,
) -> Highlighted {
    let Ok(ops) = state.0.parse_line(line, syntaxes) else {
        return (Vec::new(), None, Vec::new());
    };

    // Goto Symbol: the first text under an `entity.name.*` scope (HTML tags excluded).
    let entity_name = Scope::new("entity.name").expect("valid scope");
    let entity_tag = Scope::new("entity.name.tag").expect("valid scope");
    let mut symbol = String::new();
    let mut scopes: Vec<ScopeSpan> = Vec::new();
    let mut stack = state.1.path.clone();
    let mut last = 0;
    let mut take = |stack: &ScopeStack, range: Range<usize>| {
        let is_symbol = stack
            .as_slice()
            .iter()
            .any(|s| entity_name.is_prefix_of(*s) && !entity_tag.is_prefix_of(*s));
        if is_symbol && symbol.is_empty() {
            symbol.push_str(line[range.clone()].trim());
        }
        let scope = stack.as_slice().iter().map(|s| s.build_string()).collect::<Vec<_>>().join(" ");
        match scopes.last_mut() {
            Some(previous) if previous.scope == scope => previous.range.end = range.end,
            _ => scopes.push(ScopeSpan { range, scope }),
        }
    };
    for (pos, op) in &ops {
        if *pos > last {
            take(&stack, last..*pos);
        }
        let _ = stack.apply(op);
        last = last.max(*pos);
    }
    if last < line.len() {
        take(&stack, last..line.len());
    }

    let spans = RangedHighlightIterator::new(&mut state.1, &ops, line, highlighter)
        .map(|(style, _, range)| (range, style))
        .collect();
    (spans, (!symbol.is_empty()).then_some(symbol), scopes)
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque, as Sublime gives colors to plugins.
fn hex(c: Color) -> String {
    if c.a == 0xff {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
    }
}

pub fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn to_syntect_color(c: Rgba) -> Color {
    Color { r: c.r, g: c.g, b: c.b, a: c.a }
}

/// Converts a `.sublime-color-scheme` into a syntect theme.
fn to_syntect_theme(scheme: &ColorSchemeDefinition) -> Theme {
    let global = |name: &str| scheme.globals.get(name).copied().map(to_syntect_color);
    let settings = ThemeSettings {
        foreground: global("foreground"),
        background: global("background"),
        caret: global("caret"),
        line_highlight: global("line_highlight"),
        misspelling: global("misspelling"),
        minimap_border: global("minimap_border"),
        accent: global("accent"),
        bracket_contents_foreground: global("bracket_contents_foreground"),
        brackets_foreground: global("brackets_foreground"),
        brackets_background: global("brackets_background"),
        tags_foreground: global("tags_foreground"),
        selection: global("selection"),
        selection_foreground: global("selection_foreground"),
        selection_border: global("selection_border"),
        inactive_selection: global("inactive_selection"),
        inactive_selection_foreground: global("inactive_selection_foreground"),
        gutter: global("gutter"),
        gutter_foreground: global("gutter_foreground"),
        find_highlight: global("find_highlight"),
        find_highlight_foreground: global("find_highlight_foreground"),
        guide: global("guide"),
        active_guide: global("active_guide"),
        stack_guide: global("stack_guide"),
        highlight: global("highlight"),
        shadow: global("shadow"),
        ..Default::default()
    };
    let scopes = scheme
        .rules
        .iter()
        .filter_map(|rule| {
            let mut font_style = FontStyle::empty();
            font_style.set(FontStyle::BOLD, rule.bold);
            font_style.set(FontStyle::ITALIC, rule.italic);
            font_style.set(FontStyle::UNDERLINE, rule.underline);
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(&rule.scope).ok()?,
                style: StyleModifier {
                    foreground: rule.foreground.map(to_syntect_color),
                    background: rule.background.map(to_syntect_color),
                    font_style: rule.has_font_style.then_some(font_style),
                },
            })
        })
        .collect();
    Theme {
        name: Some(scheme.name.clone()),
        author: scheme.author.clone(),
        settings,
        scopes,
    }
}

fn find_syntax<'a>(syntaxes: &'a SyntaxSet, name: &str) -> &'a SyntaxReference {
    syntaxes
        .find_syntax_by_name(name)
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
}

/// The resource Sublime's own copy of a built-in syntax has (plugins test its file name).
fn builtin_syntax_path(name: &str) -> String {
    match name {
        "Plain Text" => "Packages/Text/Plain text.tmLanguage".into(),
        name => format!("Packages/{name}/{name}.sublime-syntax"),
    }
}

/// Patches `base` with another color scheme of the same file name: the globals it sets override
/// the base's, and its rules go last (so they win ties, as Sublime's appended patches do).
fn merge_theme(base: &mut Theme, patch: Theme) {
    merge_settings(&mut base.settings, patch.settings);
    base.scopes.extend(patch.scopes);
}

fn merge_settings(base: &mut ThemeSettings, patch: ThemeSettings) {
    macro_rules! override_set {
        ($($field:ident),* $(,)?) => {
            $(if patch.$field.is_some() {
                base.$field = patch.$field;
            })*
        };
    }
    override_set!(
        foreground,
        background,
        caret,
        line_highlight,
        misspelling,
        minimap_border,
        accent,
        popup_css,
        phantom_css,
        bracket_contents_foreground,
        bracket_contents_options,
        brackets_foreground,
        brackets_background,
        brackets_options,
        tags_foreground,
        tags_options,
        highlight,
        find_highlight,
        find_highlight_foreground,
        gutter,
        gutter_foreground,
        selection,
        selection_foreground,
        selection_border,
        inactive_selection,
        inactive_selection_foreground,
        guide,
        active_guide,
        stack_guide,
        shadow,
    );
}

/// A color's hue and saturation (0–1), for [`scheme_palette`](SyntaxEngine::scheme_palette).
fn hue_saturation(color: Color) -> (f32, f32) {
    let [r, g, b] = [color.r, color.g, color.b].map(|channel| channel as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let hue = if chroma == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    };
    let lightness = (max + min) / 2.0;
    let saturation = if chroma == 0.0 { 0.0 } else { chroma / (1.0 - (2.0 * lightness - 1.0).abs()) };
    (hue.rem_euclid(360.0), saturation)
}

/// The color of `candidates` closest to `target` degrees of hue, greys counting less.
fn closest_hue(candidates: &[Color], target: f32) -> Option<Color> {
    let distance = |color: &Color| {
        let (hue, saturation) = hue_saturation(*color);
        let gap = (hue - target).abs();
        gap.min(360.0 - gap) + (1.0 - saturation.clamp(0.0, 1.0)) * 30.0
    };
    candidates
        .iter()
        .copied()
        .filter(|color| color.a > 0)
        .min_by(|a, b| distance(a).partial_cmp(&distance(b)).unwrap_or(std::cmp::Ordering::Equal))
}

fn find_scheme<'a>(schemes: &'a BTreeMap<String, Theme>, name: &str) -> &'a Theme {
    schemes
        .get(name)
        .or_else(|| schemes.get(DEFAULT_SCHEME))
        .or_else(|| schemes.values().next())
        .expect("there is always a built-in color scheme")
}

fn file_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}
