//! Sublime Text's minihtml, the HTML of popups and phantoms, read into lines of styled text.
//!
//! Only what the text needs: bold, italic, underline, monospace (`code`, `tt`, `pre`), links
//! (`<a href>`), colors from `color` / `background-color` (in `style`, or `<font color>`), line
//! breaks (`br` and the blocks: `p`, `div`, headings, `li`…). `<style>` and `<script>` contents are
//! skipped, other tags kept for their text. Whitespace collapses outside `pre`.

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub monospace: bool,
    /// `href` of the link it is in.
    pub link: Option<String>,
    /// As written (`#rrggbb`, `#rgb`, `red`, `var(--bluish)`…); the view reads the ones it can.
    pub color: Option<String>,
    pub background: Option<String>,
    /// Inside a heading (`h1`…`h6`).
    pub heading: bool,
}

/// One line of text: the spans in order (none for an empty line).
pub type Line = Vec<Span>;

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    monospace: bool,
    preformatted: bool,
    heading: bool,
    link: Option<String>,
    color: Option<String>,
    background: Option<String>,
}

const BLOCKS: &[&str] = &[
    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "pre", "blockquote", "table", "tr", "body", "html", "hr",
];
const VOID: &[&str] = &["br", "img", "hr", "input", "meta", "link"];

/// The lines of `html`, without empty lines at the start and end.
pub fn parse(html: &str) -> Vec<Line> {
    let mut parser = Parser { lines: vec![Vec::new()], stack: vec![("".into(), Style::default())], pending_space: false };
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
        } else if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            let end = tag_end(rest);
            let tag = &rest[1..end.saturating_sub(1).max(1)];
            rest = &rest[end..];
            if let Some(skipped) = parser.tag(tag) {
                // `<style>` / `<script>`: up to the closing tag.
                let close = format!("</{skipped}");
                let at = rest.to_ascii_lowercase().find(&close).unwrap_or(rest.len());
                rest = &rest[at..];
            }
        } else {
            let end = rest.char_indices().skip(1).find(|(_, c)| *c == '<').map_or(rest.len(), |(i, _)| i);
            parser.text(&decode_entities(&rest[..end]));
            rest = &rest[end..];
        }
    }
    let mut lines = parser.lines;
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    lines.drain(..first);
    lines
}

/// Just the text, lines joined with `\n` (for a plain-text fallback).
pub fn plain_text(html: &str) -> String {
    parse(html).iter().map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// Where a tag that starts `text` ends (after its `>`), quotes inside attributes respected.
fn tag_end(text: &str) -> usize {
    let mut quote = None;
    for (i, c) in text.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return i + 1,
            _ => {}
        }
    }
    text.len()
}

struct Parser {
    lines: Vec<Line>,
    stack: Vec<(String, Style)>,
    pending_space: bool,
}

impl Parser {
    fn style(&self) -> Style {
        self.stack.last().map(|(_, s)| s.clone()).unwrap_or_default()
    }

    fn line_break(&mut self) {
        self.lines.push(Vec::new());
        self.pending_space = false;
    }

    /// A block starts or ends: a new line, unless the current one is empty.
    fn block_break(&mut self) {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.line_break();
        }
    }

    /// Handles a tag (its text between `<` and `>`). Returns the name of a tag whose contents are skipped.
    fn tag(&mut self, tag: &str) -> Option<String> {
        if tag.starts_with('!') || tag.starts_with('?') {
            return None;
        }
        let closing = tag.starts_with('/');
        let body = tag.trim_start_matches('/').trim_end_matches('/').trim();
        let name: String = body.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if name.is_empty() {
            return None;
        }
        if closing {
            if let Some(at) = self.stack.iter().rposition(|(n, _)| *n == name) {
                self.stack.truncate(at.max(1));
            }
            if BLOCKS.contains(&name.as_str()) {
                self.block_break();
            }
            return None;
        }
        if name == "style" || name == "script" {
            return Some(name);
        }
        let attributes = &body[name.len()..];
        match name.as_str() {
            "br" => {
                self.line_break();
                return None;
            }
            "hr" => {
                self.block_break();
                return None;
            }
            "img" => {
                if let Some(alt) = attribute(attributes, "alt").filter(|a| !a.is_empty()) {
                    self.text(&alt);
                }
                return None;
            }
            _ => {}
        }
        if BLOCKS.contains(&name.as_str()) {
            self.block_break();
        }
        let mut style = self.style();
        match name.as_str() {
            "b" | "strong" => style.bold = true,
            "i" | "em" => style.italic = true,
            "u" => style.underline = true,
            "code" | "tt" | "kbd" | "var" | "samp" => style.monospace = true,
            "pre" => {
                style.monospace = true;
                style.preformatted = true;
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                style.bold = true;
                style.heading = true;
            }
            "a" => {
                style.link = Some(attribute(attributes, "href").unwrap_or_default());
                style.underline = true;
            }
            "font" => {
                if let Some(color) = attribute(attributes, "color") {
                    style.color = Some(color);
                }
            }
            _ => {}
        }
        if let Some(css) = attribute(attributes, "style") {
            apply_css(&mut style, &css);
        }
        if !VOID.contains(&name.as_str()) && !body.ends_with('/') {
            self.stack.push((name.clone(), style));
        }
        if name == "li" {
            let marker = Span { text: "• ".into(), ..Default::default() };
            if let Some(line) = self.lines.last_mut() {
                line.push(marker);
            }
        }
        None
    }

    fn text(&mut self, text: &str) {
        let style = self.style();
        if style.preformatted {
            for (i, piece) in text.split('\n').enumerate() {
                if i > 0 {
                    self.line_break();
                }
                self.push(piece.to_string(), &style);
            }
            return;
        }
        // ASCII whitespace only: `&nbsp;` is a space that does not collapse.
        let mut words = String::new();
        let starts_with_space = text.starts_with(|c: char| c.is_ascii_whitespace());
        let ends_with_space = text.ends_with(|c: char| c.is_ascii_whitespace());
        for (i, word) in text.split_ascii_whitespace().enumerate() {
            if i > 0 {
                words.push(' ');
            }
            words.push_str(word);
        }
        let line_has_text = self.lines.last().is_some_and(|l| !l.is_empty());
        if words.is_empty() {
            if starts_with_space && line_has_text {
                self.pending_space = true;
            }
            return;
        }
        if (self.pending_space || starts_with_space) && line_has_text {
            words.insert(0, ' ');
        }
        self.pending_space = ends_with_space;
        self.push(words, &style);
    }

    fn push(&mut self, text: String, style: &Style) {
        if text.is_empty() {
            return;
        }
        let span = Span {
            text: text.replace('\u{a0}', " "),
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            monospace: style.monospace,
            link: style.link.clone(),
            color: style.color.clone(),
            background: style.background.clone(),
            heading: style.heading,
        };
        let line = self.lines.last_mut().expect("there is always a line");
        match line.last_mut() {
            Some(last) if Span { text: String::new(), ..last.clone() } == Span { text: String::new(), ..span.clone() } => {
                last.text.push_str(&span.text);
            }
            _ => line.push(span),
        }
    }
}

/// The value of attribute `name` in `attributes` (quoted or not), entities decoded.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before_ok = at == 0 || lower[..at].ends_with(|c: char| c.is_whitespace());
        let rest = attributes[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value = rest[1..].trim_start();
        let raw = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or_default(),
            _ => value.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or_default(),
        };
        return Some(decode_entities(raw));
    }
    None
}

fn apply_css(style: &mut Style, css: &str) {
    for declaration in css.split(';') {
        let Some((property, value)) = declaration.split_once(':') else { continue };
        let (property, value) = (property.trim().to_ascii_lowercase(), value.trim().trim_end_matches("!important").trim());
        match property.as_str() {
            "color" => style.color = Some(value.to_string()),
            "background-color" | "background" => style.background = Some(value.to_string()),
            "font-weight" => style.bold = value == "bold" || value.parse::<u32>().is_ok_and(|w| w >= 600),
            "font-style" => style.italic = value == "italic",
            "text-decoration" => style.underline = value.contains("underline"),
            "font-family" => style.monospace = style.monospace || value.contains("monospace"),
            _ => {}
        }
    }
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;` / `&#39;`, `&nbsp;` and numeric references.
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..].find(|c: char| c == ';' || c.is_whitespace() || c == '&').map(|i| i + 1);
        let decoded = end.filter(|&e| rest[e..].starts_with(';')).and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ if name.starts_with("#x") || name.starts_with("#X") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, length)) => {
                out.push(c);
                rest = &rest[length..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
