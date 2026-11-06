//! Sublime Text's minihtml, the HTML of popups, phantoms and annotations, read into boxes of
//! styled text ([`parse_html`]): blocks with their CSS box (padding, margin, border, background,
//! width), tables, rules and lines of spans, a span being text or an image.
//!
//! Inline: bold, italic, underline, monospace (`code`, `tt`, `pre`), links (`<a href>`), `color`,
//! `background-color`, `font-size`, `font-family`, images (`<img src width height>`). CSS comes
//! from `<style>` rules (type, `.class`, `#id`, `*`, compound and descendant selectors, by
//! specificity) and `style` attributes; custom properties (`--name: …`, `var(--name, fallback)`)
//! are resolved when defined (the color scheme's, such as `var(--foreground)`, are left to the
//! view). `<script>` is skipped. Whitespace collapses outside `pre`.

/// A run of text (or an image) with one style.
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// `font-size`, when set (headings set one too).
    pub font_size: Option<FontSize>,
    /// An `<img>` (the span's text is its `alt`).
    pub image: Option<Image>,
}

/// A font size: absolute, or relative to the view's font size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSize {
    Px(f32),
    /// Times the font size of the view.
    Em(f32),
}

impl FontSize {
    /// In pixels, for a view whose font is `base` pixels.
    pub fn resolve(self, base: f32) -> f32 {
        match self {
            FontSize::Px(px) => px,
            FontSize::Em(em) => em * base,
        }
    }

    fn times(self, factor: f32) -> Self {
        match self {
            FontSize::Px(px) => FontSize::Px(px * factor),
            FontSize::Em(em) => FontSize::Em(em * factor),
        }
    }
}

/// An `<img>`: its `src` (`data:`, `res://Packages/…`, `file://…`) and size, when given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Image {
    pub src: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// One line of text: the spans in order (none for an empty line).
pub type Line = Vec<Span>;

/// Lengths around a box, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sides {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Sides {
    pub fn is_zero(&self) -> bool {
        *self == Sides::default()
    }
}

/// What CSS gives a block: its box.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoxStyle {
    pub padding: Sides,
    pub margin: Sides,
    pub border_width: f32,
    pub border_color: Option<String>,
    pub border_radius: f32,
    pub background: Option<String>,
    /// `width` / `max-width`, in pixels.
    pub width: Option<f32>,
}

/// A block of minihtml (the document, a `div`, a `p`, a table cell…).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub style: BoxStyle,
    pub content: Vec<Content>,
}

/// What a block holds, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Line(Line),
    Block(Block),
    /// Rows of cells.
    Table(Vec<Vec<Block>>),
    /// `<hr>`.
    Rule,
}

/// The document's boxes. Empty lines at the start and end of blocks are left out.
pub fn parse_html(html: &str) -> Block {
    let mut parser = Parser::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
        } else if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            let end = tag_end(rest);
            let tag = &rest[1..end.saturating_sub(1).max(1)];
            rest = &rest[end..];
            if let Some(raw) = parser.tag(tag) {
                // `<style>` / `<script>`: their contents up to the closing tag.
                let close = format!("</{raw}");
                let at = rest.to_ascii_lowercase().find(&close).unwrap_or(rest.len());
                if raw == "style" {
                    parser.add_style_sheet(&rest[..at]);
                }
                rest = &rest[at..];
            }
        } else {
            let end = rest.char_indices().skip(1).find(|(_, c)| *c == '<').map_or(rest.len(), |(i, _)| i);
            parser.text(&decode_entities(&rest[..end]));
            rest = &rest[end..];
        }
    }
    parser.finish()
}

/// The lines of `html`, boxes flattened (a table row is one line, its cells apart by spaces),
/// without empty lines at the start and end.
pub fn parse(html: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    flatten(&parse_html(html), &mut lines);
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    let first = lines.iter().position(|l: &Line| !l.is_empty()).unwrap_or(lines.len());
    lines.drain(..first);
    lines
}

fn flatten(block: &Block, lines: &mut Vec<Line>) {
    for content in &block.content {
        match content {
            Content::Line(line) => lines.push(line.clone()),
            Content::Block(inner) => flatten(inner, lines),
            Content::Table(rows) => {
                for row in rows {
                    let mut line: Line = Vec::new();
                    for (i, cell) in row.iter().enumerate() {
                        let mut cell_lines = Vec::new();
                        flatten(cell, &mut cell_lines);
                        if i > 0 {
                            line.push(Span { text: "  ".into(), ..Default::default() });
                        }
                        for (j, cell_line) in cell_lines.into_iter().enumerate() {
                            if j > 0 {
                                line.push(Span { text: " ".into(), ..Default::default() });
                            }
                            line.extend(cell_line);
                        }
                    }
                    lines.push(line);
                }
            }
            Content::Rule => {}
        }
    }
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

const BLOCKS: &[&str] = &[
    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "pre", "blockquote", "table", "tr", "td", "th", "body",
    "html", "dl", "dt", "dd", "section", "article", "header", "footer", "nav", "thead", "tbody", "tfoot", "caption",
];
const VOID: &[&str] = &["br", "img", "hr", "input", "meta", "link"];
/// The pixels of an `em` in box lengths (padding, margins, borders, widths), whose view font is
/// not known here.
const EM_PX: f32 = 14.0;

/// What is inherited from parent to child.
#[derive(Clone, Default)]
struct Inline {
    bold: bool,
    italic: bool,
    underline: bool,
    monospace: bool,
    preformatted: bool,
    heading: bool,
    hidden: bool,
    link: Option<String>,
    color: Option<String>,
    background: Option<String>,
    font_size: Option<FontSize>,
    /// Custom properties (`--name`), inherited.
    variables: Vec<(String, String)>,
}

/// An element open in the parser: its name, classes and id (for CSS) and its inline style.
struct Element {
    name: String,
    classes: Vec<String>,
    id: Option<String>,
    inline: Inline,
    /// Whether it opened a block (closed with it).
    block: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum BlockKind {
    Normal,
    Table,
    Row,
    Cell,
}

/// A block being read.
struct OpenBlock {
    kind: BlockKind,
    block: Block,
}

struct Rule {
    selectors: Vec<Vec<Compound>>,
    declarations: Vec<(String, String)>,
    order: usize,
}

#[derive(Default, Clone)]
struct Compound {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
}

impl Compound {
    fn matches(&self, element: &Element) -> bool {
        // The document itself is `html`, `body` and `:root`, written or not.
        let root = element.name.is_empty();
        self.tag.as_ref().is_none_or(|t| *t == element.name || (root && matches!(t.as_str(), "html" | "body" | ":root")))
            && self.id.as_ref().is_none_or(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|c| element.classes.contains(c))
    }

    fn specificity(&self) -> u32 {
        self.id.is_some() as u32 * 100 + self.classes.len() as u32 * 10 + self.tag.is_some() as u32
    }
}

struct Parser {
    root: Vec<OpenBlock>,
    elements: Vec<Element>,
    rules: Vec<Rule>,
    pending_space: bool,
}

impl Parser {
    fn new() -> Self {
        Self {
            root: vec![OpenBlock { kind: BlockKind::Normal, block: Block::default() }],
            elements: vec![Element { name: String::new(), classes: Vec::new(), id: None, inline: Inline::default(), block: false }],
            rules: Vec::new(),
            pending_space: false,
        }
    }

    fn inline(&self) -> Inline {
        self.elements.last().map(|e| e.inline.clone()).unwrap_or_default()
    }

    fn current(&mut self) -> &mut Block {
        &mut self.root.last_mut().expect("the document block stays").block
    }

    /// The line text goes to: the last one of the current block, or a new one.
    fn line(&mut self) -> &mut Line {
        let block = self.current();
        if !matches!(block.content.last(), Some(Content::Line(_))) {
            block.content.push(Content::Line(Vec::new()));
        }
        match block.content.last_mut() {
            Some(Content::Line(line)) => line,
            _ => unreachable!("a line was just added"),
        }
    }

    fn line_has_text(&self) -> bool {
        matches!(self.root.last().and_then(|b| b.block.content.last()), Some(Content::Line(l)) if !l.is_empty())
    }

    fn line_break(&mut self) {
        self.line();
        self.current().content.push(Content::Line(Vec::new()));
        self.pending_space = false;
    }

    /// A block element in the middle of text: the text goes on below.
    fn block_break(&mut self) {
        if self.line_has_text() {
            self.current().content.push(Content::Line(Vec::new()));
        }
        self.pending_space = false;
    }

    fn add_style_sheet(&mut self, css: &str) {
        let css = strip_css_comments(css);
        let mut text = css.as_str();
        while let Some(open) = text.find('{') {
            let selector = text[..open].trim();
            let Some(close) = text[open..].find('}') else { break };
            let body = &text[open + 1..open + close];
            let order = self.rules.len();
            let selectors: Vec<Vec<Compound>> = selector.split(',').filter_map(parse_selector).collect();
            if !selectors.is_empty() {
                self.rules.push(Rule { selectors, declarations: declarations(body), order });
            }
            text = &text[open + close + 1..];
        }
        // `html` / `body` / `:root` rules style the document itself (and define its variables).
        let root = &self.elements[0];
        let mut found: Vec<(u32, usize, Vec<(String, String)>)> = self
            .rules
            .iter()
            .filter_map(|rule| {
                let best = rule.selectors.iter().filter(|s| s.len() == 1 && s[0].matches(root)).map(|s| s[0].specificity()).max()?;
                Some((best, rule.order, rule.declarations.clone()))
            })
            .collect();
        found.sort_by_key(|(specificity, order, _)| (*specificity, *order));
        let mut inline = Inline::default();
        let mut box_style = BoxStyle::default();
        let mut block = false;
        let declarations: Vec<(String, String)> = found.into_iter().flat_map(|(_, _, d)| d).collect();
        for (property, value) in declarations.iter().filter(|(p, _)| p.starts_with("--")) {
            let value = resolve_variables(value, &inline.variables);
            inline.variables.push((property.clone(), value));
        }
        for (property, value) in declarations.iter().filter(|(p, _)| !p.starts_with("--")) {
            apply(property, &resolve_variables(value, &inline.variables), &mut inline, &mut box_style, &mut block);
        }
        // What the root gave is inherited by the elements already open (normally none).
        for element in &mut self.elements {
            let mut merged = inline.clone();
            merged.variables.extend(element.inline.variables.iter().cloned());
            element.inline.variables = merged.variables;
            if element.inline.color.is_none() {
                element.inline.color = inline.color.clone();
            }
            if element.inline.font_size.is_none() {
                element.inline.font_size = inline.font_size;
            }
            element.inline.monospace |= inline.monospace;
        }
        self.root[0].block.style = box_style;
    }

    /// The declarations that apply to the element on top of the stack, lowest priority first.
    fn matching_declarations(&self) -> Vec<(String, String)> {
        let mut found: Vec<(u32, usize, &Vec<(String, String)>)> = Vec::new();
        for rule in &self.rules {
            let best = rule.selectors.iter().filter(|s| self.selector_matches(s)).map(|s| s.iter().map(Compound::specificity).sum()).max();
            if let Some(specificity) = best {
                found.push((specificity, rule.order, &rule.declarations));
            }
        }
        found.sort_by_key(|(specificity, order, _)| (*specificity, *order));
        found.into_iter().flat_map(|(_, _, d)| d.iter().cloned()).collect()
    }

    /// The last compound matches the element on top; the others, ancestors in order.
    fn selector_matches(&self, selector: &[Compound]) -> bool {
        let Some((last, ancestors)) = selector.split_last() else { return false };
        let Some((element, parents)) = self.elements.split_last() else { return false };
        if !last.matches(element) {
            return false;
        }
        let mut at = parents.len();
        for compound in ancestors.iter().rev() {
            match parents[..at].iter().rposition(|e| compound.matches(e)) {
                Some(found) => at = found,
                None => return false,
            }
        }
        true
    }

    /// Handles a tag (its text between `<` and `>`). Returns the name of a tag whose contents
    /// are raw (`style`, `script`).
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
            self.close(&name);
            return None;
        }
        if name == "style" || name == "script" {
            return Some(name);
        }
        let attributes = &body[name.len()..];
        let classes = attribute(attributes, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
        let id = attribute(attributes, "id");
        // The element is on the stack while its CSS is worked out.
        let parent = self.inline();
        self.elements.push(Element { name: name.clone(), classes, id, inline: parent.clone(), block: false });
        let mut inline = parent;
        let mut box_style = BoxStyle::default();
        let mut display_block = BLOCKS.contains(&name.as_str());
        default_style(&name, attributes, &mut inline);
        let mut declarations = self.matching_declarations();
        if let Some(css) = attribute(attributes, "style") {
            declarations.extend(self::declarations(&css));
        }
        // Custom properties first: the others may use them.
        for (property, value) in &declarations {
            if property.starts_with("--") {
                let value = resolve_variables(value, &inline.variables);
                inline.variables.retain(|(n, _)| n != property);
                inline.variables.push((property.clone(), value));
            }
        }
        for (property, value) in &declarations {
            if !property.starts_with("--") {
                let value = resolve_variables(value, &inline.variables);
                apply(property, &value, &mut inline, &mut box_style, &mut display_block);
            }
        }
        let is_void = VOID.contains(&name.as_str()) || body.ends_with('/');
        match name.as_str() {
            "br" => self.line_break(),
            "hr" => {
                self.block_break();
                if !inline.hidden {
                    self.current().content.push(Content::Rule);
                }
            }
            "img" if !inline.hidden => {
                let size = |key: &str| attribute(attributes, key).and_then(|v| length(&v, None)).or_else(|| css_length(&declarations, key));
                let image = Image { src: attribute(attributes, "src").unwrap_or_default(), width: size("width"), height: size("height") };
                let alt = attribute(attributes, "alt").unwrap_or_default();
                self.push(alt, &inline, Some(image));
            }
            _ => {}
        }
        let element = self.elements.last_mut().expect("pushed above");
        element.inline = inline;
        if is_void {
            self.elements.pop();
            return None;
        }
        if display_block {
            self.open_block(&name, box_style);
            self.elements.last_mut().expect("pushed above").block = true;
        }
        if name == "li" {
            let inline = self.inline();
            self.push("• ".into(), &Inline { bold: false, underline: false, link: None, ..inline }, None);
        }
        None
    }

    fn open_block(&mut self, name: &str, style: BoxStyle) {
        let kind = match name {
            "table" => BlockKind::Table,
            "tr" => BlockKind::Row,
            "td" | "th" => BlockKind::Cell,
            _ => BlockKind::Normal,
        };
        // Text before the block stays above it.
        self.block_break();
        self.root.push(OpenBlock { kind, block: Block { style, content: Vec::new() } });
        self.pending_space = false;
    }

    fn close(&mut self, name: &str) {
        let Some(at) = self.elements.iter().rposition(|e| e.name == name) else { return };
        if at == 0 {
            return;
        }
        while self.elements.len() > at {
            let element = self.elements.pop().expect("checked above");
            if element.block {
                self.close_block();
            }
        }
    }

    /// Ends the innermost block and puts it in its parent (a row's cells, a table's rows).
    fn close_block(&mut self) {
        if self.root.len() < 2 {
            return;
        }
        let OpenBlock { kind, mut block, .. } = self.root.pop().expect("checked above");
        trim_lines(&mut block.content);
        let parent = self.root.last_mut().expect("checked above");
        match kind {
            BlockKind::Table => {
                let rows = block
                    .content
                    .into_iter()
                    .filter_map(|c| match c {
                        Content::Table(rows) => Some(rows),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                parent.block.content.push(Content::Table(rows));
            }
            BlockKind::Row => {
                let cells: Vec<Block> = block
                    .content
                    .into_iter()
                    .filter_map(|c| match c {
                        Content::Block(cell) => Some(cell),
                        _ => None,
                    })
                    .collect();
                // A row goes into its table (through `thead` / `tbody`) as a one-row table.
                parent.block.content.push(Content::Table(vec![cells]));
            }
            // `thead` / `tbody` in a table: their rows are the table's.
            BlockKind::Normal if parent.kind == BlockKind::Table => {
                parent.block.content.extend(block.content.into_iter().filter(|c| matches!(c, Content::Table(_))));
            }
            BlockKind::Cell | BlockKind::Normal => parent.block.content.push(Content::Block(block)),
        }
        self.pending_space = false;
    }

    fn text(&mut self, text: &str) {
        let inline = self.inline();
        if inline.hidden {
            return;
        }
        // Text between table elements (indentation) is not content.
        if matches!(self.root.last().map(|b| b.kind), Some(BlockKind::Table | BlockKind::Row)) {
            return;
        }
        if inline.preformatted {
            for (i, piece) in text.split('\n').enumerate() {
                if i > 0 {
                    self.line_break();
                }
                self.push(piece.to_string(), &inline, None);
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
        let line_has_text = self.line_has_text();
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
        self.push(words, &inline, None);
    }

    fn push(&mut self, text: String, style: &Inline, image: Option<Image>) {
        if text.is_empty() && image.is_none() {
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
            font_size: style.font_size,
            image,
        };
        let line = self.line();
        match line.last_mut() {
            Some(last) if span.image.is_none() && last.image.is_none() && same_style(last, &span) => last.text.push_str(&span.text),
            _ => line.push(span),
        }
    }

    fn finish(mut self) -> Block {
        while self.root.len() > 1 {
            self.close_block();
        }
        let mut block = self.root.pop().expect("the document block").block;
        trim_lines(&mut block.content);
        block
    }
}

fn same_style(a: &Span, b: &Span) -> bool {
    Span { text: String::new(), ..a.clone() } == Span { text: String::new(), ..b.clone() }
}

/// Empty lines at the start and end of a block are not content.
fn trim_lines(content: &mut Vec<Content>) {
    while matches!(content.last(), Some(Content::Line(l)) if l.is_empty()) {
        content.pop();
    }
    let first = content.iter().position(|c| !matches!(c, Content::Line(l) if l.is_empty())).unwrap_or(content.len());
    content.drain(..first);
}

/// What an element is before CSS: bold for `b`, a link for `a`, sizes of headings…
fn default_style(name: &str, attributes: &str, inline: &mut Inline) {
    let base = inline.font_size.unwrap_or(FontSize::Em(1.0));
    match name {
        "b" | "strong" | "th" => inline.bold = true,
        "i" | "em" => inline.italic = true,
        "u" => inline.underline = true,
        "code" | "tt" | "kbd" | "var" | "samp" => inline.monospace = true,
        "pre" => {
            inline.monospace = true;
            inline.preformatted = true;
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            inline.bold = true;
            inline.heading = true;
            let scale = match name {
                "h1" => 1.5,
                "h2" => 1.3,
                "h3" => 1.17,
                "h4" => 1.0,
                "h5" => 0.83,
                _ => 0.67,
            };
            inline.font_size = Some(base.times(scale));
        }
        "small" => inline.font_size = Some(base.times(0.85)),
        "big" => inline.font_size = Some(base.times(1.2)),
        "a" => {
            inline.link = Some(attribute(attributes, "href").unwrap_or_default());
            inline.underline = true;
        }
        "font" => {
            if let Some(color) = attribute(attributes, "color") {
                inline.color = Some(color);
            }
        }
        _ => {}
    }
}

/// Applies one CSS declaration.
fn apply(property: &str, value: &str, inline: &mut Inline, box_style: &mut BoxStyle, display_block: &mut bool) {
    let parent_size = inline.font_size.unwrap_or(FontSize::Em(1.0));
    match property {
        "color" => inline.color = Some(value.to_string()),
        "background-color" => {
            inline.background = Some(value.to_string());
            box_style.background = Some(value.to_string());
        }
        "background" => {
            let color = value.split_whitespace().find(|v| !v.starts_with("url(")).unwrap_or(value);
            inline.background = Some(color.to_string());
            box_style.background = Some(color.to_string());
        }
        "font-weight" => inline.bold = value == "bold" || value == "bolder" || value.parse::<u32>().is_ok_and(|w| w >= 600),
        "font-style" => inline.italic = value == "italic" || value == "oblique",
        "text-decoration" | "text-decoration-line" => inline.underline = value.contains("underline"),
        "font-family" => inline.monospace = value.contains("monospace") || value.contains("Courier") || value.contains("Consolas"),
        "font-size" => {
            if let Some(size) = font_size(value, parent_size) {
                inline.font_size = Some(size);
            }
        }
        "display" => match value {
            "none" => inline.hidden = true,
            "block" | "list-item" | "table" => *display_block = true,
            "inline" | "inline-block" => *display_block = false,
            _ => {}
        },
        "white-space" => inline.preformatted = value.starts_with("pre"),
        "padding" => box_style.padding = sides(value),
        "margin" => box_style.margin = sides(value),
        "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => set_side(&mut box_style.padding, property, value),
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => set_side(&mut box_style.margin, property, value),
        "border" | "border-top" | "border-bottom" | "border-left" | "border-right" => {
            for part in value.split_whitespace() {
                if let Some(width) = length(part, None) {
                    box_style.border_width = width;
                } else if !matches!(part, "solid" | "dashed" | "dotted" | "double" | "none") {
                    box_style.border_color = Some(part.to_string());
                }
            }
            if value.trim() == "none" {
                box_style.border_width = 0.0;
            }
        }
        "border-width" => box_style.border_width = length(value, None).unwrap_or(0.0),
        "border-color" => box_style.border_color = Some(value.to_string()),
        "border-radius" => box_style.border_radius = length(value.split_whitespace().next().unwrap_or(value), None).unwrap_or(0.0),
        "width" | "max-width" => box_style.width = length(value, None),
        _ => {}
    }
}

/// `margin` / `padding` shorthands: one to four lengths.
fn sides(value: &str) -> Sides {
    let values: Vec<f32> = value.split_whitespace().map(|v| length(v, None).unwrap_or(0.0)).collect();
    match values.as_slice() {
        [all] => Sides { top: *all, right: *all, bottom: *all, left: *all },
        [vertical, horizontal] => Sides { top: *vertical, right: *horizontal, bottom: *vertical, left: *horizontal },
        [top, horizontal, bottom] => Sides { top: *top, right: *horizontal, bottom: *bottom, left: *horizontal },
        [top, right, bottom, left, ..] => Sides { top: *top, right: *right, bottom: *bottom, left: *left },
        [] => Sides::default(),
    }
}

fn set_side(sides: &mut Sides, property: &str, value: &str) {
    let v = length(value, None).unwrap_or(0.0);
    match property.rsplit('-').next() {
        Some("top") => sides.top = v,
        Some("right") => sides.right = v,
        Some("bottom") => sides.bottom = v,
        _ => sides.left = v,
    }
}

/// A CSS length in pixels (`4px`, `0.5em`, `1rem`, `3pt`, `0`); `percent_of` for `%`.
fn length(value: &str, percent_of: Option<f32>) -> Option<f32> {
    let value = value.trim();
    let number = |suffix: &str| value.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    if let Some(px) = number("px") {
        Some(px)
    } else if let Some(em) = number("rem").or_else(|| number("em")) {
        Some(em * EM_PX)
    } else if let Some(pt) = number("pt") {
        Some(pt * 4.0 / 3.0)
    } else if let Some(percent) = number("%") {
        percent_of.map(|of| of * percent / 100.0)
    } else {
        value.parse::<f32>().ok()
    }
}

/// The `width` / `height` of an image given in its CSS.
fn css_length(declarations: &[(String, String)], key: &str) -> Option<f32> {
    declarations.iter().rev().find(|(p, _)| p == key).and_then(|(_, v)| length(v, None))
}

/// `font-size`: lengths, `em` / `%` of the parent's, `rem` of the view's, keywords.
fn font_size(value: &str, parent: FontSize) -> Option<FontSize> {
    let value = value.trim();
    let number = |suffix: &str| value.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    if let Some(px) = number("px") {
        return Some(FontSize::Px(px));
    }
    if let Some(rem) = number("rem") {
        return Some(FontSize::Em(rem));
    }
    if let Some(em) = number("em") {
        return Some(parent.times(em));
    }
    if let Some(pt) = number("pt") {
        return Some(FontSize::Px(pt * 4.0 / 3.0));
    }
    if let Some(percent) = number("%") {
        return Some(parent.times(percent / 100.0));
    }
    let scale = match value {
        "xx-small" => 0.6,
        "x-small" => 0.75,
        "small" => 0.89,
        "medium" => 1.0,
        "large" => 1.2,
        "x-large" => 1.5,
        "xx-large" => 2.0,
        "smaller" => return Some(parent.times(0.83)),
        "larger" => return Some(parent.times(1.2)),
        _ => return None,
    };
    Some(FontSize::Em(scale))
}

/// `var(--name)` / `var(--name, fallback)` replaced by the custom property's value when it is
/// defined (or its fallback); others are left for the view (the color scheme's variables).
fn resolve_variables(value: &str, variables: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find("var(") {
        out.push_str(&rest[..at]);
        let inner_start = at + 4;
        let Some(close) = matching_paren(&rest[inner_start..]) else {
            out.push_str(&rest[at..]);
            return out;
        };
        let inner = &rest[inner_start..inner_start + close];
        let (name, fallback) = match inner.split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (inner.trim(), None),
        };
        match variables.iter().rev().find(|(n, _)| n == name) {
            Some((_, defined)) => out.push_str(defined),
            None if fallback.is_some() && !is_scheme_variable(name) => out.push_str(&resolve_variables(fallback.unwrap_or_default(), variables)),
            None => out.push_str(&rest[at..inner_start + close + 1]),
        }
        rest = &rest[inner_start + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The variables the view knows from the color scheme (`--foreground`, `--redish`…).
fn is_scheme_variable(name: &str) -> bool {
    let name = name.trim_start_matches("--");
    matches!(name, "foreground" | "background" | "accent") || name.ends_with("ish")
}

/// Where the `)` that closes the text's first `(`-level ends, nested parentheses respected.
fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn strip_css_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        rest = rest[at + 2..].find("*/").map_or("", |end| &rest[at + 2 + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// `property: value; …` (properties lower case, `!important` dropped).
fn declarations(css: &str) -> Vec<(String, String)> {
    css.split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim();
            let property = if property.starts_with("--") { property.to_string() } else { property.to_ascii_lowercase() };
            Some((property, value.trim().trim_end_matches("!important").trim().to_string()))
        })
        .filter(|(p, _)| !p.is_empty())
        .collect()
}

/// A selector: compounds from the outermost ancestor to the element (`>` read as a descendant).
fn parse_selector(text: &str) -> Option<Vec<Compound>> {
    let mut compounds = Vec::new();
    for part in text.replace('>', " ").split_whitespace() {
        let mut compound = Compound::default();
        let mut current = String::new();
        let mut kind = 't';
        let flush = |kind: char, current: &mut String, compound: &mut Compound| {
            if current.is_empty() {
                return;
            }
            let value = std::mem::take(current);
            match kind {
                '.' => compound.classes.push(value),
                '#' => compound.id = Some(value),
                _ if value != "*" => compound.tag = Some(value.to_ascii_lowercase()),
                _ => {}
            }
        };
        if part == ":root" {
            compounds.push(Compound { tag: Some(":root".into()), ..Compound::default() });
            continue;
        }
        for c in part.chars() {
            match c {
                '.' | '#' => {
                    flush(kind, &mut current, &mut compound);
                    kind = c;
                }
                // Pseudo-classes and attributes are not supported: no match.
                ':' | '[' => return None,
                c => current.push(c),
            }
        }
        flush(kind, &mut current, &mut compound);
        compounds.push(compound);
    }
    (!compounds.is_empty()).then_some(compounds)
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

/// The bytes of a `data:` URI (base64 or percent-encoded); `None` for other sources.
pub fn data_uri_bytes(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.ends_with(";base64") {
        decode_base64(data)
    } else {
        Some(percent_decode(data))
    }
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').map(value).collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, d)| n | (u32::from(*d) << (18 - 6 * i)));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(byte) = text.get(i + 1..i + 3).and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}
