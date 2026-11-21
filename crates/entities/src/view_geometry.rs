//! Entities: the geometry of a view as the plugin API sees it (`view.text_to_layout`,
//! `layout_to_text`, `window_to_layout`, `viewport_position`, `line_height`, `em_width`…).
//!
//! Three coordinate spaces, in pixels, as in Sublime:
//! - *layout*: the whole text laid out, (0, 0) at the top left of its first char;
//! - *window*: the view on screen, (0, 0) at its top left (gutter included);
//! - the *viewport* is the part of the layout shown: it starts at `viewport_position`.
//!
//! The text is monospace: a char takes `em_width` (a tab `tab_width` of them, as the view draws
//! it) and a line `line_height`. With word wrap, a line breaks after the last space that fits in
//! `wrap_columns` (anywhere when no space does), as the view does.

/// What the view measured when it last drew the text; [`Default`] until it has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewGeometry {
    pub line_height: f32,
    pub em_width: f32,
    /// The layout point at the top left of what is shown (the scroll offset).
    pub viewport_position: (f32, f32),
    /// The size of what is shown.
    pub viewport_extent: (f32, f32),
    /// Where layout (0, 0) is in window coordinates when nothing is scrolled (after the gutter).
    pub text_origin: (f32, f32),
    /// Columns a tab takes.
    pub tab_width: usize,
    /// Columns per row when the view wraps lines.
    pub wrap_columns: Option<usize>,
}

impl Default for ViewGeometry {
    fn default() -> Self {
        Self {
            line_height: 16.0,
            em_width: 8.0,
            viewport_position: (0.0, 0.0),
            viewport_extent: (800.0, 600.0),
            text_origin: (0.0, 0.0),
            tab_width: 4,
            wrap_columns: None,
        }
    }
}

impl ViewGeometry {
    /// The layout position of the char at `point` (its top left).
    pub fn text_to_layout(&self, text: &str, point: usize) -> (f32, f32) {
        let (row, column) = self.visual_position(text, point);
        (column as f32 * self.em_width, row as f32 * self.line_height)
    }

    /// The char nearest to a layout position (the end of a row past its last char).
    pub fn layout_to_text(&self, text: &str, (x, y): (f32, f32)) -> usize {
        let row = (y / self.line_height).floor().max(0.0) as usize;
        let column = (x / self.em_width).max(0.0);
        let rows = self.rows(text);
        let Some(&(start, end)) = rows.get(row).or(rows.last()) else {
            return 0;
        };
        if row >= rows.len() {
            return end;
        }
        let mut at = 0.0;
        for (index, c) in text.chars().skip(start).take(end - start).enumerate() {
            let width = self.char_columns(c) as f32;
            // Before the middle of a char: that char; past it: the next.
            if column < at + width / 2.0 {
                return start + index;
            }
            at += width;
        }
        end
    }

    /// Width and height of the whole text laid out.
    pub fn layout_extent(&self, text: &str) -> (f32, f32) {
        let rows = self.rows(text);
        let widest = rows
            .iter()
            .map(|&(start, end)| self.columns(text, start, end))
            .max()
            .unwrap_or(0);
        (
            widest as f32 * self.em_width,
            rows.len().max(1) as f32 * self.line_height,
        )
    }

    pub fn window_to_layout(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            x - self.text_origin.0 + self.viewport_position.0,
            y - self.text_origin.1 + self.viewport_position.1,
        )
    }

    pub fn layout_to_window(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            x + self.text_origin.0 - self.viewport_position.0,
            y + self.text_origin.1 - self.viewport_position.1,
        )
    }

    /// The row drawn that holds `point` (a char range without its line break), when the view
    /// wraps lines; `None` when it does not (a row is then the whole line). A point where a
    /// wrapped row ends and the next begins belongs to the next.
    pub fn wrapped_row(&self, text: &str, point: usize) -> Option<(usize, usize)> {
        self.wrap_columns.filter(|&l| l > 0)?;
        let rows = self.rows(text);
        let row = rows.iter().rposition(|&(start, _)| start <= point)?;
        rows.get(row).copied()
    }

    /// The (row, column) a point is drawn at, rows counting wrapped ones.
    fn visual_position(&self, text: &str, point: usize) -> (usize, usize) {
        let rows = self.rows(text);
        let row = rows
            .iter()
            .rposition(|&(start, _)| start <= point)
            .unwrap_or(0);
        let (start, end) = rows.get(row).copied().unwrap_or((0, 0));
        (row, self.columns(text, start, point.min(end)))
    }

    /// The rows drawn, as char ranges without their line break: a line each, or more with wrap.
    fn rows(&self, text: &str) -> Vec<(usize, usize)> {
        let mut rows = Vec::new();
        let mut line_start = 0;
        let mut line: Vec<char> = Vec::new();
        for c in text.chars().chain(std::iter::once('\n')) {
            if c == '\n' {
                self.wrap_line(&line, line_start, &mut rows);
                line_start += line.len() + 1;
                line.clear();
            } else {
                line.push(c);
            }
        }
        rows
    }

    fn wrap_line(&self, line: &[char], offset: usize, rows: &mut Vec<(usize, usize)>) {
        let Some(limit) = self.wrap_columns.filter(|&l| l > 0) else {
            rows.push((offset, offset + line.len()));
            return;
        };
        let mut start = 0;
        loop {
            let mut width = 0;
            let mut end = start;
            let mut last_space = None;
            while end < line.len() {
                let w = self.char_columns(line[end]);
                if width + w > limit && end > start {
                    break;
                }
                width += w;
                if line[end].is_whitespace() {
                    last_space = Some(end);
                }
                end += 1;
            }
            if end >= line.len() {
                rows.push((offset + start, offset + line.len()));
                return;
            }
            let cut = last_space.map_or(end, |space| space + 1);
            rows.push((offset + start, offset + cut));
            start = cut;
        }
    }

    fn columns(&self, text: &str, start: usize, end: usize) -> usize {
        text.chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|c| self.char_columns(c))
            .sum()
    }

    fn char_columns(&self, c: char) -> usize {
        if c == '\t' {
            self.tab_width
        } else {
            1
        }
    }
}
