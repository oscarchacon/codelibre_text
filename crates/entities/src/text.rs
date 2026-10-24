//! Position helpers over a `&str`.
//!
//! Selections are char indices, but Rust strings are edited with byte offsets, so text commands
//! convert with [`char_to_byte`] / [`byte_to_char`].

use crate::selection::Selection;

pub fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

/// Byte offset of the start of the line containing `byte`.
pub fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the end of the line containing `byte` (before its newline).
pub fn line_end(text: &str, byte: usize) -> usize {
    text[byte..].find('\n').map(|i| byte + i).unwrap_or(text.len())
}

/// Zero-based (line, column) of a char index.
pub fn line_col(text: &str, char_index: usize) -> (usize, usize) {
    let byte = char_to_byte(text, char_index);
    let line = text[..byte].matches('\n').count();
    let column = text[line_start(text, byte)..byte].chars().count();
    (line, column)
}

/// Char index of the start of a zero-based line (clamped to the end of the text).
pub fn line_to_char(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    match text.match_indices('\n').nth(line - 1) {
        Some((byte, _)) => byte_to_char(text, byte + 1),
        None => text.chars().count(),
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word around a char index, as a selection (empty if there is no word there).
pub fn word_at(text: &str, position: usize) -> Selection {
    let chars: Vec<char> = text.chars().collect();
    let mut start = position.min(chars.len());
    let mut end = start;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    Selection::new(start, end)
}

/// The selected substring.
pub fn selected_text(text: &str, selection: Selection) -> &str {
    &text[char_to_byte(text, selection.start())..char_to_byte(text, selection.end())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_chars_and_bytes() {
        let text = "año\nb";
        assert_eq!(char_to_byte(text, 2), 3);
        assert_eq!(byte_to_char(text, 3), 2);
    }

    #[test]
    fn reports_line_and_column() {
        assert_eq!(line_col("ab\ncd", 4), (1, 1));
        assert_eq!(line_to_char("ab\ncd\nef", 2), 6);
    }

    #[test]
    fn finds_word_around_caret() {
        assert_eq!(word_at("let foo_bar = 1", 6), Selection::new(4, 11));
    }
}
