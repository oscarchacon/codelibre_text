//! syntect (a Rust port of Sublime Text's highlighting engine): syntax definitions, color schemes
//! and per-document highlighting caches.
//!
//! One instance is shared (`Rc<RefCell<…>>`) by the [`SyntaxGateway`](use_cases::gateways::SyntaxGateway)
//! implementation, which the use cases call, and by the editor view, which paints with it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use egui::text::{LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, FontId, Stroke};
use entities::document::DocumentId;
use entities::symbol::Symbol;
use interface_adapters::gateways::color_scheme_file::{self, ColorSchemeDefinition, Rgba};
use interface_adapters::gateways::{builtin_resources, package_files};
use syntect::highlighting::{
    Color, FontStyle, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, Style, StyleModifier,
    Theme, ThemeItem, ThemeSet, ThemeSettings,
};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet};
use use_cases::gateways::PackagePaths;

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
        for path in package_files::find_resources(&self.paths, include_sublime, |n| n.ends_with(".sublime-syntax")) {
            let definition = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| {
                let fallback_name = path.file_stem().and_then(|s| s.to_str());
                SyntaxDefinition::load_from_str(&text, true, fallback_name).map_err(|e| e.to_string())
            });
            match definition {
                Ok(definition) => builder.add(definition),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        self.syntaxes = builder.build();

        self.load_builtin_schemes(errors);
        for path in package_files::find_resources(&self.paths, include_sublime, |n| {
            n.ends_with(".tmTheme") || n.ends_with(".sublime-color-scheme")
        }) {
            let name = file_stem(&path);
            let theme = if path.extension().is_some_and(|e| e == "tmTheme") {
                ThemeSet::get_theme(&path).map_err(|e| e.to_string())
            } else {
                std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|source| color_scheme_file::parse(&source, &name))
                    .map(|definition| to_syntect_theme(&definition))
            };
            match theme {
                Ok(theme) => {
                    self.schemes.insert(name, theme);
                }
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

    pub fn base_scope(&self, syntax: &str) -> String {
        find_syntax(&self.syntaxes, syntax).scope.to_string()
    }

    pub fn syntax_names(&self) -> Vec<String> {
        self.syntaxes.syntaxes().iter().filter(|s| !s.hidden).map(|s| s.name.clone()).collect()
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

    pub fn symbols(&mut self, document: DocumentId, text: &str, syntax: &str) -> Vec<Symbol> {
        let syntax = self.syntax(syntax).clone();
        let theme = self.scheme(&self.current_scheme).clone();
        let cache = self.caches.entry(document).or_default();
        cache.update(text, &self.syntaxes, &syntax, &theme);
        cache.symbols()
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
        cache.update(text, &self.syntaxes, syntax, theme);
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

#[derive(Clone)]
struct CachedLine {
    text: String,
    state_before: (ParseState, HighlightState),
    spans: Vec<(Range<usize>, Style)>,
    symbol: Option<String>,
}

impl HighlightCache {
    fn symbols(&self) -> Vec<Symbol> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(line, l)| l.symbol.as_ref().map(|name| Symbol { line, name: name.clone() }))
            .collect()
    }

    fn update(&mut self, text: &str, syntaxes: &SyntaxSet, syntax: &SyntaxReference, theme: &Theme) {
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
        let mut state = match (old.get(unchanged), unchanged.checked_sub(1).and_then(|i| old.get(i))) {
            (Some(line), _) => line.state_before.clone(),
            (None, Some(previous)) => {
                // Appending after the last cached line: continue from its end state.
                let mut s = previous.state_before.clone();
                highlight_line(&previous.text, &mut s, syntaxes, &highlighter);
                s
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
            if old_index >= unchanged as isize {
                if let Some(old_line) = old.get(old_index as usize) {
                    if old_line.text == line_text && old_line.state_before == state {
                        lines.extend_from_slice(&old[old_index as usize..]);
                        break;
                    }
                }
            }
            let state_before = state.clone();
            let (spans, symbol) = highlight_line(line_text, &mut state, syntaxes, &highlighter);
            lines.push(CachedLine {
                text: line_text.to_string(),
                state_before,
                spans,
                symbol,
            });
        }
        self.lines = lines;
    }
}

fn highlight_line(
    line: &str,
    state: &mut (ParseState, HighlightState),
    syntaxes: &SyntaxSet,
    highlighter: &Highlighter,
) -> (Vec<(Range<usize>, Style)>, Option<String>) {
    let Ok(ops) = state.0.parse_line(line, syntaxes) else {
        return (Vec::new(), None);
    };

    // Goto Symbol: the first text under an `entity.name.*` scope (HTML tags excluded).
    let entity_name = Scope::new("entity.name").expect("valid scope");
    let entity_tag = Scope::new("entity.name.tag").expect("valid scope");
    let mut symbol = String::new();
    let mut stack = state.1.path.clone();
    let mut last = 0;
    let mut take = |stack: &ScopeStack, piece: &str| {
        let is_symbol = stack
            .as_slice()
            .iter()
            .any(|s| entity_name.is_prefix_of(*s) && !entity_tag.is_prefix_of(*s));
        if is_symbol && symbol.is_empty() {
            symbol.push_str(piece.trim());
        }
    };
    for (pos, op) in &ops {
        if *pos > last {
            take(&stack, &line[last..*pos]);
        }
        let _ = stack.apply(op);
        last = *pos;
    }
    if last < line.len() {
        take(&stack, &line[last..]);
    }

    let spans = RangedHighlightIterator::new(&mut state.1, &ops, line, highlighter)
        .map(|(style, _, range)| (range, style))
        .collect();
    (spans, (!symbol.is_empty()).then_some(symbol))
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
        selection: global("selection"),
        selection_foreground: global("selection_foreground"),
        selection_border: global("selection_border"),
        inactive_selection: global("inactive_selection"),
        gutter: global("gutter"),
        gutter_foreground: global("gutter_foreground"),
        find_highlight: global("find_highlight"),
        find_highlight_foreground: global("find_highlight_foreground"),
        guide: global("guide"),
        active_guide: global("active_guide"),
        accent: global("accent"),
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
