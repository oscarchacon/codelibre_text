//! Reads `.sublime-color-scheme` files (Sublime Text 3.1+ JSON color schemes) into a plain
//! [`ColorSchemeDefinition`]. The highlighting engine in the outer circle turns it into its own theme type.
//!
//! Supports `variables`, `globals`, `rules`, hex / `rgb()` / `hsl()` colors and `color(<c> alpha(<a>))`.

use std::collections::HashMap;

use serde_json::Value;

use super::sublime_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ColorRule {
    /// Scope selector, e.g. `comment, punctuation.definition.comment`.
    pub scope: String,
    pub foreground: Option<Rgba>,
    pub background: Option<Rgba>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// True when the rule sets `font_style` (even to an empty value).
    pub has_font_style: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ColorSchemeDefinition {
    pub name: String,
    pub author: Option<String>,
    /// Global colors by Sublime's names (`background`, `foreground`, `caret`, `line_highlight`, …).
    pub globals: HashMap<String, Rgba>,
    pub rules: Vec<ColorRule>,
}

pub fn parse(source: &str, fallback_name: &str) -> Result<ColorSchemeDefinition, String> {
    let value = sublime_json::parse(source).map_err(|e| e.to_string())?;
    let variables: HashMap<String, String> = value
        .get("variables")
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let colors = ColorResolver { variables };

    let globals = value
        .get("globals")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), colors.resolve(v.as_str()?)?)))
                .collect()
        })
        .unwrap_or_default();

    let rules = value
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|rule| {
            let color = |key: &str| match rule.get(key)? {
                Value::String(s) => colors.resolve(s),
                // Gradient foregrounds: use the first stop.
                Value::Array(stops) => colors.resolve(stops.first()?.as_str()?),
                _ => None,
            };
            let font_style = rule.get("font_style").and_then(Value::as_str);
            let has = |style: &str| font_style.is_some_and(|f| f.split_whitespace().any(|s| s == style));
            Some(ColorRule {
                scope: rule.get("scope")?.as_str()?.to_string(),
                foreground: color("foreground"),
                background: color("background"),
                bold: has("bold"),
                italic: has("italic"),
                underline: has("underline"),
                has_font_style: font_style.is_some(),
            })
        })
        .collect();

    Ok(ColorSchemeDefinition {
        name: value.get("name").and_then(Value::as_str).unwrap_or(fallback_name).to_string(),
        author: value.get("author").and_then(Value::as_str).map(String::from),
        globals,
        rules,
    })
}

struct ColorResolver {
    variables: HashMap<String, String>,
}

impl ColorResolver {
    fn resolve(&self, spec: &str) -> Option<Rgba> {
        self.resolve_at_depth(spec, 0)
    }

    fn resolve_at_depth(&self, spec: &str, depth: usize) -> Option<Rgba> {
        if depth > 16 {
            return None; // circular variables
        }
        let spec = spec.trim();
        if let Some(name) = spec.strip_prefix("var(").and_then(|s| s.strip_suffix(')')) {
            return self.resolve_at_depth(self.variables.get(name.trim())?, depth + 1);
        }
        if let Some(inner) = spec.strip_prefix("color(").and_then(|s| s.strip_suffix(')')) {
            return self.color_function(inner, depth);
        }
        parse_literal(spec)
    }

    /// `color(<base> alpha(<a>))` and `color(<base> a(<a>))`; other adjusters are ignored.
    fn color_function(&self, inner: &str, depth: usize) -> Option<Rgba> {
        let base_end = if inner.starts_with("var(") || inner.starts_with("rgb") || inner.starts_with("hsl") {
            inner.find(')')? + 1
        } else {
            inner.find(char::is_whitespace).unwrap_or(inner.len())
        };
        let mut color = self.resolve_at_depth(&inner[..base_end], depth + 1)?;
        let rest = &inner[base_end..];
        for prefix in ["alpha(", "a("] {
            if let Some(pos) = rest.find(prefix) {
                let start = pos + prefix.len();
                if let Some(alpha) = rest[start..].find(')').and_then(|len| parse_alpha(&rest[start..start + len])) {
                    color.a = alpha;
                }
                break;
            }
        }
        Some(color)
    }
}

fn parse_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    let value = match s.strip_suffix('%') {
        Some(percent) => percent.trim().parse::<f32>().ok()? / 100.0,
        None => s.parse::<f32>().ok()?,
    };
    Some((value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, `hsl()`, `hsla()` and a few names.
fn parse_literal(spec: &str) -> Option<Rgba> {
    if let Some(hex) = spec.strip_prefix('#') {
        let bytes: Vec<u8> = match hex.len() {
            3 | 4 => hex.chars().map(|c| u8::from_str_radix(&format!("{c}{c}"), 16).ok()).collect::<Option<_>>()?,
            6 | 8 => (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
                .collect::<Option<_>>()?,
            _ => return None,
        };
        return Some(Rgba { r: bytes[0], g: bytes[1], b: bytes[2], a: bytes.get(3).copied().unwrap_or(255) });
    }
    let args = |prefix: &str| -> Option<Vec<String>> {
        let inner = spec.strip_prefix(prefix)?.trim_start_matches('a').trim();
        let inner = inner.strip_prefix('(')?.strip_suffix(')')?;
        Some(inner.split(',').map(|s| s.trim().to_string()).collect())
    };
    let alpha = |a: &[String]| a.get(3).and_then(|s| parse_alpha(s)).unwrap_or(255);
    if let Some(a) = args("rgb") {
        let channel = |i: usize| a.get(i)?.parse::<f32>().ok().map(|v| v as u8);
        return Some(Rgba { r: channel(0)?, g: channel(1)?, b: channel(2)?, a: alpha(&a) });
    }
    if let Some(a) = args("hsl") {
        let percent = |i: usize| a.get(i)?.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0);
        let hue = a.first()?.trim_end_matches("deg").parse::<f32>().ok()?;
        let (r, g, b) = hsl_to_rgb(hue, percent(1)?, percent(2)?);
        return Some(Rgba { r, g, b, a: alpha(&a) });
    }
    match spec {
        "white" => Some(Rgba { r: 255, g: 255, b: 255, a: 255 }),
        "black" => Some(Rgba { r: 0, g: 0, b: 0, a: 255 }),
        "transparent" => Some(Rgba { r: 0, g: 0, b: 0, a: 0 }),
        _ => None,
    }
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    let to_byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_byte(r), to_byte(g), to_byte(b))
}
