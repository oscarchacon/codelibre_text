//! Scope selectors, as in Sublime Text: what key bindings (`selector` context), build systems,
//! color schemes and plugins match against the scope of a point (`source.python string.quoted`).

/// How well `selector` matches `scope` (space-separated scope names, outermost first): 0 when it
/// does not. `,` or `|` separate alternatives, ` - ` excludes, a space means a scope inside the
/// previous one. Deeper and longer matches score more. An empty selector matches everything.
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

/// Every element of `path` matches a scope after the one the previous element matched.
fn path_score(scopes: &[&str], path: &str) -> u64 {
    let mut score = 0u64;
    let mut position = 0;
    let mut elements = path.split_whitespace().peekable();
    if elements.peek().is_none() {
        return 1;
    }
    for element in elements {
        let found = scopes[position..].iter().position(|s| *s == element || s.strip_prefix(element).is_some_and(|r| r.starts_with('.')));
        let Some(offset) = found else { return 0 };
        position += offset;
        let depth = element.matches('.').count() as u64 + 1;
        score = score.saturating_add(depth << (3 * position.min(20)));
        position += 1;
    }
    score
}
