//! Scope selectors, as in Sublime Text: what key bindings (`selector` context), build systems,
//! color schemes and plugins match against the scope of a point (`source.python string.quoted`).

/// How well `selector` matches `scope` (space-separated scope names, outermost first): 0 when it
/// does not. `,` or `|` separate alternatives (the best one counts), `&` asks for both, `-`
/// excludes (`source - string`, or alone: `-comment`), parentheses group
/// (`source - (string | comment)`), and a space means a scope inside the previous one. The score of
/// a path is what Sublime Text 4215 answers (checked against it on 279 cases, see
/// `entities/tests/selector.rs`): each selector element adds the number of its dotted parts that
/// matched (at most 7) shifted to bit `3 × position + 1` of the scope element it matched, the last
/// element at the rightmost scope that matches it and each one before at the rightmost before
/// that. So `a` on `a` is 2, on `x a` 16, on `s a b a b a` 65536. An empty selector matches
/// everything with 1, as does an exclusion alone that does not apply. Only the innermost 21
/// scopes have slots of their own: in a deeper stack the outer ones share the first.
///
/// Sublime stops reading at a term followed by another with no operator between them (`(a) b`,
/// `a (b)`) or at a stray `)`: what follows is not read, not even `, c`, and a parenthesis left
/// open then (or never closed: `(a`) makes the selector match nothing. So does an empty
/// alternative (`, a`, `a,,c`, `(a,)`, `()`) but a separator ending the selector (`a,`, `a |`), and
/// a `-` with nothing after it. `||` and `&&` are `|` and `&`.
pub fn score_selector(scope: &str, selector: &str) -> u64 {
    let scopes: Vec<&str> = scope.split_whitespace().collect();
    let tokens = tokens(selector);
    if tokens.is_empty() {
        return 1;
    }
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        scopes: &scopes,
        stopped: false,
        failed: false,
    };
    let score = parser.alternatives(0);
    if parser.failed {
        0
    } else {
        score
    }
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
    fn finish<'a>(
        selector: &'a str,
        tokens: &mut Vec<Token<'a>>,
        start: &mut Option<usize>,
        end: usize,
    ) {
        if let Some(begin) = start.take() {
            tokens.push(Token::Name(&selector[begin..end]));
        }
    }
    let mut tokens = Vec::new();
    let mut name_start: Option<usize> = None;
    let mut previous = None;
    for (at, c) in selector.char_indices() {
        // `||` and `&&` are one operator each (`,,` is two: an empty alternative).
        let doubled = matches!(c, '|' | '&') && previous == Some(c);
        previous = Some(c);
        if doubled {
            continue;
        }
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
/// An empty alternative makes the selector match nothing (see [`score_selector`]).
struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    at: usize,
    scopes: &'t [&'t str],
    /// A term was followed by another with no operator: nothing after it is read.
    stopped: bool,
    /// A parenthesis was not closed: the selector matches nothing.
    failed: bool,
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.at).copied()
    }

    /// `depth` is how many parentheses are open: only at 0 may a separator end the selector.
    fn alternatives(&mut self, depth: usize) -> u64 {
        let Some(mut best) = self.intersection(depth) else {
            return self.fail();
        };
        while !self.stopped && self.peek() == Some(Token::Or) {
            self.at += 1;
            match self.intersection(depth) {
                Some(score) => best = best.max(score),
                None if depth == 0 && self.peek().is_none() => break,
                None => return self.fail(),
            }
        }
        best
    }

    /// The selector matches nothing, and nothing more is read.
    fn fail(&mut self) -> u64 {
        self.failed = true;
        self.stopped = true;
        0
    }

    /// `None` when there is nothing before the next `,`, `|` or `)`.
    fn intersection(&mut self, depth: usize) -> Option<u64> {
        let mut score = self.term(depth)?;
        loop {
            if self.stopped {
                return Some(score);
            }
            match self.peek() {
                Some(Token::And) => {
                    self.at += 1;
                    let other = self.term(depth).unwrap_or(1);
                    score = if score > 0 && other > 0 {
                        score.max(other)
                    } else {
                        0
                    };
                }
                Some(Token::Minus) => {
                    self.at += 1;
                    match self.term(depth) {
                        None => return Some(self.fail()),
                        Some(excluded) if excluded > 0 => score = 0,
                        Some(_) => {}
                    }
                }
                // Two terms side by side, as in `(a) b`: Sublime reads no further.
                Some(Token::Name(_)) | Some(Token::Open) => {
                    self.stopped = true;
                    return Some(score);
                }
                _ => return Some(score),
            }
        }
    }

    fn term(&mut self, depth: usize) -> Option<u64> {
        match self.peek()? {
            Token::Minus => {
                self.at += 1;
                // An exclusion alone: it matches what it does not exclude.
                Some(match self.term(depth) {
                    None => self.fail(),
                    Some(excluded) if excluded > 0 => 0,
                    Some(_) => 1,
                })
            }
            Token::Open => {
                self.at += 1;
                let score = self.alternatives(depth + 1);
                if self.stopped || self.peek() != Some(Token::Close) {
                    return Some(self.fail());
                }
                self.at += 1;
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

/// Every element of `path` matches a scope before the one the next element matched, from the
/// last element back, each at the rightmost scope that matches it (which finds a match whenever
/// there is one, and the best). The score is 64 bits: the innermost 21 scopes have a slot each.
fn path_score(scopes: &[&str], path: &[&str]) -> u64 {
    if path.is_empty() {
        return 1;
    }
    let mut score = 0u64;
    let mut end = scopes.len();
    let outer = scopes.len().saturating_sub(21);
    for element in path.iter().rev() {
        let found =
            scopes[..end].iter().enumerate().rev().find_map(
                |(position, scope)| match element_score(scope, element) {
                    0 => None,
                    matched => Some((position, matched)),
                },
            );
        let Some((position, matched)) = found else {
            return 0;
        };
        let shift = 3 * position.saturating_sub(outer) as u32 + 1;
        score = score.wrapping_add(matched.checked_shl(shift).unwrap_or(0));
        end = position;
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
