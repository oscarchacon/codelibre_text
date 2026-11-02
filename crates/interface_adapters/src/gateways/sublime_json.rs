//! Sublime Text's JSON dialect: standard JSON plus `//` and `/* */` comments and trailing commas.

use serde_json::Value;

pub fn parse(source: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(&to_strict_json(source))
}

/// Removes comments and trailing commas (outside of strings).
fn to_strict_json(source: &str) -> String {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut in_string = false;

    // Index just after the comment starting at `i`, if there is one.
    let skip_comment = |i: usize| -> Option<usize> {
        match (chars.get(i), chars.get(i + 1)) {
            (Some('/'), Some('/')) => Some((i..chars.len()).find(|&j| chars[j] == '\n').unwrap_or(chars.len())),
            (Some('/'), Some('*')) => {
                let mut j = i + 2;
                while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '/') {
                    j += 1;
                }
                Some((j + 2).min(chars.len()))
            }
            _ => None,
        }
    };

    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            in_string = c != '"';
            i += 1;
            continue;
        }
        if let Some(next) = skip_comment(i) {
            i = next;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            ',' => {
                // Keep the comma unless the next significant character closes a container.
                let mut j = i + 1;
                loop {
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    match skip_comment(j) {
                        Some(next) => j = next,
                        None => break,
                    }
                }
                if !matches!(chars.get(j), Some('}') | Some(']')) {
                    out.push(',');
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}
