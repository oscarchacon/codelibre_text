//! Scope selectors, as in Sublime Text: what key bindings (`selector` context), build systems,
//! color schemes and plugins match against the scope of a point (`source.python string.quoted`).

/// How well `selector` matches `scope` (space-separated scope names, outermost first): 0 when it
/// does not. `,` or `|` separate alternatives, ` - ` excludes, a space means a scope inside the
/// previous one. The score is Sublime's (reverse-engineered from sublimehq/sublime_text#2152): each
/// selector element adds, at the 3-bit slot of the scope element it matched, the number of its
/// dotted parts that matched (at most 7). So `a` on `a` is 8, `a` on `x a` is 64. An empty
/// selector matches everything with 1.
pub fn score_selector(scope: &str, selector: &str) -> u64 {
    let scopes: Vec<&str> = scope.split_whitespace().collect();
    let selector = selector.replace(['(', ')'], " ");
    if selector.trim().is_empty() {
        return 1;
    }
    let mut best = 0;
    for alternative in selector.split([',', '|']).map(str::trim).filter(|a| !a.is_empty()) {
        let (wanted, excluded) = match alternative.strip_prefix('-') {
            Some(rest) => ("", rest),
            None => alternative.split_once(" - ").unwrap_or((alternative, "")),
        };
        let score = path_score(&scopes, wanted);
        let excluded = excluded.split(" - ").any(|e| !e.trim().is_empty() && path_score(&scopes, e) > 0);
        if score > 0 && !excluded {
            best = best.max(score);
        }
    }
    best
}

/// Whether `selector` matches `scope` at all.
pub fn matches_selector(scope: &str, selector: &str) -> bool {
    score_selector(scope, selector) > 0
}

/// Every element of `path` matches a scope after the one the previous element matched, each one
/// at the leftmost scope that matches it (Sublime does not backtrack). The score is 64 bits: the
/// slot of a scope element is its 1-based position, which Sublime caps at 20.
fn path_score(scopes: &[&str], path: &str) -> u64 {
    let mut elements = path.split_whitespace().peekable();
    if elements.peek().is_none() {
        return 1;
    }
    let mut score = 0u64;
    let mut position = 0;
    for element in elements {
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
