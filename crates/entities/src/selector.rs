//! Scope selectors, as in Sublime Text: what key bindings (`selector` context), build systems,
//! color schemes and plugins match against the scope of a point (`source.python string.quoted`).

/// How well `selector` matches `scope` (space-separated scope names, outermost first): 0 when it
/// does not. `,` or `|` separate alternatives (the best one counts), `&` asks for both, `-`
/// excludes (`source - string`, or alone: `-comment`), parentheses group
/// (`source - (string | comment)`), and a space means a scope inside the previous one. The score of
/// a path is Sublime's (reverse-engineered from sublimehq/sublime_text#2152): each selector element
/// adds, at the 3-bit slot of the scope element it matched, the number of its dotted parts that
/// matched (at most 7). So `a` on `a` is 8, `a` on `x a` is 64. An empty selector matches
/// everything with 1, as does an exclusion alone that does not apply.
pub fn score_selector(scope: &str, selector: &str) -> u64 {
    let scopes: Vec<&str> = scope.split_whitespace().collect();
    let tokens = tokens(selector);
    if tokens.is_empty() {
        return 1;
    }
    let mut parser = Parser { tokens: &tokens, at: 0, scopes: &scopes };
    let mut score = parser.alternatives();
    // A stray `)` ends the selector early: what follows is read as more alternatives, so a
    // selector with unbalanced parentheses still matches what it names.
    while parser.at < tokens.len() {
        parser.at += 1;
        score = score.max(parser.alternatives());
    }
    score
}

/// Whether `selector` matches `scope` at all.
pub fn matches_selector(scope: &str, selector: &str) -> bool {
    score_selector(scope, selector) > 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    /// A scope name of a path (`string.quoted`).
    Name(&'a str),
    /// `,` or `|`.
    Or,
    And,
    Minus,
    Open,
    Close,
}

/// The selector's tokens. A `-` is an operator where a name cannot go on: at the start, or after a
/// space, a parenthesis or another operator; inside a name it is part of it (`text.html.markdown-gfm`).
fn tokens(selector: &str) -> Vec<Token<'_>> {
    fn finish<'a>(selector: &'a str, tokens: &mut Vec<Token<'a>>, start: &mut Option<usize>, end: usize) {
        if let Some(begin) = start.take() {
            tokens.push(Token::Name(&selector[begin..end]));
        }
    }
    let mut tokens = Vec::new();
    let mut name_start: Option<usize> = None;
    for (at, c) in selector.char_indices() {
        let operator = match c {
            ',' | '|' => Some(Token::Or),
            '&' => Some(Token::And),
            '(' => Some(Token::Open),
            ')' => Some(Token::Close),
            '-' if name_start.is_none() => Some(Token::Minus),
            _ => None,
        };
        if let Some(operator) = operator {
            finish(selector, &mut tokens, &mut name_start, at);
            tokens.push(operator);
        } else if c.is_whitespace() {
            finish(selector, &mut tokens, &mut name_start, at);
        } else if name_start.is_none() {
            name_start = Some(at);
        }
    }
    finish(selector, &mut tokens, &mut name_start, selector.len());
    tokens
}

/// Reads the tokens and scores them on the way (recursive descent):
/// `alternatives := intersection (("," | "|") intersection)*`,
/// `intersection := term (("&" | "-") term)*`, `term := "-" term | "(" alternatives ")" | path`.
/// An empty alternative or group is skipped: `a, , b` is `a, b`.
struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    at: usize,
    scopes: &'t [&'t str],
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.at).copied()
    }

    fn alternatives(&mut self) -> u64 {
        let mut best = self.intersection().unwrap_or(0);
        while self.peek() == Some(Token::Or) {
            self.at += 1;
            best = best.max(self.intersection().unwrap_or(0));
        }
        best
    }

    /// `None` when there is nothing before the next `,`, `|` or `)`.
    fn intersection(&mut self) -> Option<u64> {
        let mut score = self.term()?;
        loop {
            match self.peek() {
                Some(Token::And) => {
                    self.at += 1;
                    let other = self.term().unwrap_or(1);
                    score = if score > 0 && other > 0 { score.max(other) } else { 0 };
                }
                Some(Token::Minus) => {
                    self.at += 1;
                    if self.term().is_some_and(|excluded| excluded > 0) {
                        score = 0;
                    }
                }
                // Two terms side by side, as in `(a) b`: both must match.
                Some(Token::Name(_)) | Some(Token::Open) => {
                    let other = self.term().unwrap_or(1);
                    score = if score > 0 && other > 0 { score.max(other) } else { 0 };
                }
                _ => return Some(score),
            }
        }
    }

    fn term(&mut self) -> Option<u64> {
        match self.peek()? {
            Token::Minus => {
                self.at += 1;
                // An exclusion alone: it matches what it does not exclude.
                Some(match self.term() {
                    Some(excluded) if excluded > 0 => 0,
                    _ => 1,
                })
            }
            Token::Open => {
                self.at += 1;
                let score = self.alternatives();
                if self.peek() == Some(Token::Close) {
                    self.at += 1;
                }
                Some(score)
            }
            Token::Name(_) => {
                let mut path = Vec::new();
                while let Some(Token::Name(name)) = self.peek() {
                    path.push(name);
                    self.at += 1;
                }
                Some(path_score(self.scopes, &path))
            }
            Token::Or | Token::And | Token::Close => None,
        }
    }
}

/// Every element of `path` matches a scope after the one the previous element matched, each one
/// at the leftmost scope that matches it (Sublime does not backtrack). The score is 64 bits: the
/// slot of a scope element is its 1-based position, which Sublime caps at 20.
fn path_score(scopes: &[&str], path: &[&str]) -> u64 {
    if path.is_empty() {
        return 1;
    }
    let mut score = 0u64;
    let mut position = 0;
    for element in path {
        let found = scopes[position..].iter().enumerate().find_map(|(offset, scope)| match element_score(scope, element) {
            0 => None,
            matched => Some((offset, matched)),
        });
        let Some((offset, matched)) = found else { return 0 };
        position += offset;
        score = score.wrapping_add(matched << (3 * (position + 1).min(20)));
        position += 1;
    }
    score
}

/// How many of the dotted parts of `element` match the start of `scope` (`source.python` on
/// `source.python.x` is 2), at most 7; 0 when the first part differs or `element` has more parts
/// than `scope`.
fn element_score(scope: &str, element: &str) -> u64 {
    let mut scope_parts = scope.split('.');
    let mut matched = 0;
    for part in element.split('.') {
        if scope_parts.next() != Some(part) {
            return 0;
        }
        matched += 1;
    }
    matched.min(7)
}
