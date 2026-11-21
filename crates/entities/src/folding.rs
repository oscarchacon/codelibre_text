//! Entities: what code folding folds (Sublime's `fold`, `fold_by_level`), by indentation.
//!
//! A block is the run of lines indented more than the line before it (its header), blank lines
//! inside it included. Its fold goes from the end of the header to the end of its last line that
//! is not blank, so the header stays visible and the line after the block starts a row of its own.
//! Positions are chars.

/// The fold of the block at the caret's line: the block it heads, else the block it is in.
pub fn fold_region_at(text: &str, point: usize, tab_size: usize) -> Option<(usize, usize)> {
    let lines = Lines::new(text, tab_size);
    let line = lines.line_of(point);
    let header = if lines.heads_block(line) {
        line
    } else {
        let indent = lines.indent_of_block_line(line)?;
        (0..line)
            .rev()
            .find(|&l| !lines.blank(l) && lines.indent(l) < indent)?
    };
    lines.block_fold(header)
}

/// The folds of `fold_by_level`: every block whose header is at indentation `level - 1`
/// (level 1 folds the bodies of the outermost blocks).
pub fn folds_at_level(text: &str, level: usize, tab_size: usize) -> Vec<(usize, usize)> {
    let lines = Lines::new(text, tab_size);
    let wanted = level.saturating_sub(1) * tab_size.max(1);
    (0..lines.count())
        .filter(|&l| !lines.blank(l) && lines.indent(l) == wanted && lines.heads_block(l))
        .filter_map(|l| lines.block_fold(l))
        .collect()
}

/// The lines from `first` to `last` (0-based, both included) that head a block, for the gutter's
/// fold buttons: those `fold_region_at` folds from their start.
pub fn fold_headers(text: &str, first: usize, last: usize, tab_size: usize) -> Vec<usize> {
    let lines = Lines::new(text, tab_size);
    (first..=last.min(lines.count().saturating_sub(1)))
        .filter(|&l| lines.heads_block(l))
        .collect()
}

/// The folds of `fold_tag_attributes`: in each tag, from the end of its name to the `>` (or `/>`)
/// that closes it, as Sublime 4215 folds them (checked by running it). Found by scope, so only
/// syntaxes with tags (HTML, XML) have any: `tokens` are the text's pieces (chars) and their
/// scopes, in order. Tags without attributes, `<?xml …?>` and comments have none; a fold keeps
/// the spaces and line breaks around the attributes.
pub fn tag_attribute_folds(text: &str, tokens: &[(usize, usize, &str)]) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let has = |scope: &str, part: &str| scope.split_whitespace().any(|s| s == part || s.starts_with(&format!("{part}.")));
    let mut folds = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let (_, _, scope) = tokens[index];
        index += 1;
        if !has(scope, "entity.name.tag") || !has(scope, "meta.tag") || has(scope, "meta.tag.preprocessor") {
            continue;
        }
        // The name may be in pieces (`x:y`): the fold starts after its last one.
        while index < tokens.len() && has(tokens[index].2, "entity.name.tag") {
            index += 1;
        }
        let start = tokens[index - 1].1;
        let Some(close) = tokens[index..]
            .iter()
            .position(|t| !has(t.2, "meta.tag") || has(t.2, "punctuation.definition.tag.end"))
            .map(|at| index + at)
        else {
            break;
        };
        index = close;
        if !has(tokens[close].2, "punctuation.definition.tag.end") {
            continue;
        }
        // Older HTML syntaxes (syntect's own) scope the space before `/>` as part of it; Sublime
        // 4's does not, and the fold keeps that space.
        let (close_start, close_end) = (tokens[close].0, tokens[close].1.min(chars.len()));
        let end = close_start
            + chars[close_start.min(close_end)..close_end]
                .iter()
                .take_while(|c| c.is_whitespace())
                .count();
        let inside = chars.get(start..end.min(chars.len())).unwrap_or_default();
        if inside.iter().any(|c| !c.is_whitespace()) {
            folds.push((start, end));
        }
    }
    folds
}

/// Folds sorted, without empty ones, those that overlap or touch merged into one.
pub fn merge_folds(folds: &mut Vec<(usize, usize)>) {
    folds.retain(|&(a, b)| a < b);
    folds.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(folds.len());
    for &(a, b) in folds.iter() {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    *folds = merged;
}

/// The lines of a text: where each starts and ends (chars, without its line break), and how far
/// it is indented (columns, a tab reaching the next multiple of `tab_size`).
struct Lines {
    spans: Vec<(usize, usize)>,
    indents: Vec<Option<usize>>,
}

impl Lines {
    fn new(text: &str, tab_size: usize) -> Self {
        let tab_size = tab_size.max(1);
        let mut spans = Vec::new();
        let mut indents = Vec::new();
        let mut start = 0;
        for line in text.split('\n') {
            let len = line.chars().count();
            spans.push((start, start + len));
            let mut columns = 0;
            let mut indent = None;
            for c in line.chars() {
                match c {
                    ' ' => columns += 1,
                    '\t' => columns = (columns / tab_size + 1) * tab_size,
                    _ => {
                        indent = Some(columns);
                        break;
                    }
                }
            }
            indents.push(indent);
            start += len + 1;
        }
        Self { spans, indents }
    }

    fn count(&self) -> usize {
        self.spans.len()
    }

    fn line_of(&self, point: usize) -> usize {
        self.spans
            .iter()
            .rposition(|&(start, _)| start <= point)
            .unwrap_or(0)
    }

    fn blank(&self, line: usize) -> bool {
        self.indents[line].is_none()
    }

    fn indent(&self, line: usize) -> usize {
        self.indents[line].unwrap_or(0)
    }

    /// A blank line counts as indented like the next line that is not.
    fn indent_of_block_line(&self, line: usize) -> Option<usize> {
        (line..self.count()).find_map(|l| self.indents[l])
    }

    fn next_non_blank(&self, line: usize) -> Option<usize> {
        (line + 1..self.count()).find(|&l| !self.blank(l))
    }

    fn heads_block(&self, line: usize) -> bool {
        !self.blank(line)
            && self
                .next_non_blank(line)
                .is_some_and(|next| self.indent(next) > self.indent(line))
    }

    fn block_fold(&self, header: usize) -> Option<(usize, usize)> {
        let indent = self.indent(header);
        let mut last = None;
        for (line, line_indent) in self.indents.iter().enumerate().skip(header + 1) {
            match *line_indent {
                None => {}
                Some(i) if i > indent => last = Some(line),
                Some(_) => break,
            }
        }
        let last = last?;
        Some((self.spans[header].1, self.spans[last].1))
    }
}
