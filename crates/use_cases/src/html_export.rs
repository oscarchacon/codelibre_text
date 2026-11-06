//! Use cases: `view.export_to_html` — the text as HTML, each run in a span with the color
//! scheme's style for its scopes (`Packages/…/Save as HTML`-like plugins read it).
//!
//! The text goes in a `<pre>` so its lines and spaces stay as they are; with `enclosing_tags` a
//! `<div>` around it carries the scheme's colors (and the font size and family asked for), as in
//! Sublime. `minihtml` output is the same: our minihtml parser reads `<pre>`, `<div>` and `style`
//! attributes.

use entities::document::DocumentId;
use entities::selection::Selection;
use entities::text::char_to_byte;
use serde_json::Value;

use crate::editor::Editor;

/// The options of `view.export_to_html`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HtmlExportOptions {
    /// Make the HTML readable by our minihtml (the shapes used here already are).
    pub minihtml: bool,
    /// Wrap the text in a `<div>` with the scheme's colors; without it no background color is set.
    pub enclosing_tags: bool,
    /// Put the view's font size in that `<div>`.
    pub font_size: bool,
    /// Put `monospace` as the font family in that `<div>`.
    pub font_family: bool,
}

impl Editor {
    /// `view.export_to_html(regions, minihtml, enclosing_tags, font_size, font_family)`: the text
    /// of the regions (the whole view without any) as HTML. `None` when the view is closed.
    pub fn export_to_html(
        &self,
        id: DocumentId,
        regions: Option<&[Selection]>,
        options: HtmlExportOptions,
    ) -> Option<String> {
        let doc = self.any_document(id)?;
        let scheme = self.color_scheme_name();
        let base = self.gateways.syntaxes.scheme_style(&scheme);
        let foreground = base.get("foreground").and_then(Value::as_str).unwrap_or("#000000");
        let background = base.get("background").and_then(Value::as_str).unwrap_or("#ffffff");

        let ranges: Vec<(usize, usize)> = match regions {
            Some(regions) if !regions.is_empty() => {
                regions.iter().map(|s| (s.anchor.min(s.head), s.anchor.max(s.head))).collect()
            }
            _ => vec![(0, doc.char_count())],
        };
        let mut runs: Vec<(String, String)> = Vec::new();
        for (index, (start, end)) in ranges.into_iter().enumerate() {
            if index > 0 {
                // The regions are one after the other, as Sublime joins them.
                runs.push((String::new(), "\n".to_string()));
            }
            for token in self.scoped_ranges(id, start, end) {
                let text = &doc.text[char_to_byte(&doc.text, token.start)..char_to_byte(&doc.text, token.end)];
                let css = self.span_css(&scheme, &token.scope, foreground);
                match runs.last_mut() {
                    Some((last_css, last_text)) if *last_css == css => last_text.push_str(&escape_html(text)),
                    _ => runs.push((css, escape_html(text))),
                }
            }
        }
        let mut body = String::new();
        for (css, text) in runs {
            if css.is_empty() || text.is_empty() {
                body.push_str(&text);
            } else {
                body.push_str(&format!("<span style=\"{css}\">{text}</span>"));
            }
        }
        let body = format!("<pre style=\"margin: 0;\">{body}</pre>");
        if !options.enclosing_tags {
            return Some(body);
        }
        let mut div = vec![format!("color: {foreground};"), format!("background-color: {background};")];
        if options.font_family {
            div.push("font-family: monospace;".to_string());
        }
        if options.font_size {
            let size = self.settings_of(id).f32("font_size", 11.0);
            let size = if size.fract() == 0.0 { format!("{size:.0}") } else { size.to_string() };
            div.push(format!("font-size: {size}pt;"));
        }
        Some(format!("<div style=\"{}\">{body}</div>", div.join(" ")))
    }

    /// The `style` of the span of one scope: its color, background and decorations in the scheme.
    fn span_css(&self, scheme: &str, scope: &str, default_foreground: &str) -> String {
        let style = self.gateways.syntaxes.scope_style(scheme, scope);
        let foreground = style.get("foreground").and_then(Value::as_str).unwrap_or(default_foreground);
        let mut css = vec![format!("color: {foreground};")];
        if let Some(background) = style.get("background").and_then(Value::as_str) {
            css.push(format!("background-color: {background};"));
        }
        if style.get("bold").and_then(Value::as_bool).unwrap_or(false) {
            css.push("font-weight: bold;".to_string());
        }
        if style.get("italic").and_then(Value::as_bool).unwrap_or(false) {
            css.push("font-style: italic;".to_string());
        }
        if style.get("underline").and_then(Value::as_bool).unwrap_or(false) {
            css.push("text-decoration: underline;".to_string());
        }
        css.join(" ")
    }
}

/// The text as HTML: `&`, `<` and `>` escaped (quotes need not be, in text).
fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(c),
        }
    }
    escaped
}
