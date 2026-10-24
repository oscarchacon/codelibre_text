//! Multi-select (Ctrl+D) - Buscar y seleccionar ocurrencias
//! Supports:
//! - Ctrl+D: Select current word and add cursors to next occurrences
//! - Ctrl+Shift+D: Duplicate line (existing shortcut)
//! - Ctrl+K Ctrl+D: Move selected occurrence to previous

use crate::domain::text_buffer::{Position, Cursor, TextBuffer};
use uuid::Uuid;

/// State for Ctrl+D multi-select across all buffers
#[derive(Debug)]
pub struct MultiSelectState {
    /// Selected cursor positions (all active cursors)
    pub cursors: Vec<Position>,
    /// The word/text that was originally selected (for finding next)
    pub selection_text: Option<String>,
    /// Starting position of the original selection
    pub anchor_position: usize,
    /// Whether multi-select mode is active
    pub active: bool,
}

impl MultiSelectState {
    pub fn new() -> Self {
        Self {
            cursors: Vec::new(),
            selection_text: None,
            anchor_position: 0,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn clear(&mut self) {
        self.cursors.clear();
        self.selection_text = None;
        self.active = false;
    }
}

/// Ctrl+D: Expand selection to next occurrence of word under cursor
/// Returns the updated list of cursors (positions)
pub fn ctrl_d_expand_selection(
    buffer: &mut TextBuffer,
    multi_select: &mut MultiSelectState,
) {
    let content = buffer.content();
    let cursors = buffer.cursors().to_vec();

    if let Some(cursor) = cursors.first() {
        let pos = cursor.position.0;

        // Get the word under cursor
        let word = extract_word_at(content, pos);

        if word.is_empty() {
            return;
        }

        // If this is the first Ctrl+D press, record the word and add current cursor
        if !multi_select.active {
            multi_select.selection_text = Some(word.clone());
            multi_select.anchor_position = pos;
            multi_select.cursors.clear();
            multi_select.cursors.push(cursor.position);
            multi_select.active = true;
        } else {
            // Find next occurrence starting after the last cursor position
            if let Some(start_pos) = find_next_occurrence(content, &word, pos + word.len()) {
                let new_pos = Position(start_pos);
                multi_select.cursors.push(new_pos);

                // Update all cursors to select the word
                let mut new_cursors = Vec::new();
                for cursor in &cursors {
                    // Find matching cursor positions
                    if let Some(idx) = multi_select.cursors.iter().position(|&p| {
                        if p.0 >= cursor.position.0 - word.len() && p.0 <= cursor.position.0 + word.len() {
                            // Check if the text at that position matches the word
                            let end = std::cmp::min(p.0 + word.len(), content.len());
                            if p.0 < content.len() {
                                content[p.0..end] == word
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }) {
                        new_cursors.push(Cursor::new(multi_select.cursors[idx]));
                    } else {
                        // Keep existing cursors that don't match
                        new_cursors.push(cursor.clone());
                    }
                }

                // Add new cursor for the found occurrence
                new_cursors.push(Cursor::new(new_pos));
                buffer.set_cursors(new_cursors);
            }
        }
    }
}

/// Ctrl+Shift+D: Duplicate line at each cursor
pub fn ctrl_shift_d_duplicate_line(buffer: &mut TextBuffer) {
    let cursors = buffer.cursors().to_vec();

    // Collect line texts and positions in reverse order to maintain positions
    let mut insertions: Vec<(usize, String)> = Vec::new();

    for cursor in &cursors {
        let pos = cursor.position.0;
        let content = buffer.content();
        let line_start = get_line_start(content, pos);
        let line_end = get_line_end(content, pos);

        if line_start < line_end && line_end <= content.len() {
            let line_text = content[line_start..line_end].to_string();
            insertions.push((line_end, line_text));
        }
    }

    // Insert in reverse order to maintain positions
    insertions.sort_by(|a, b| b.0.cmp(&a.0));
    let content_mut = buffer.content_mut();
    for (pos, text) in insertions {
        if pos < content_mut.len() {
            let mut insert_pos = pos;
            // Insert after the newline, or at end
            if content_mut.as_bytes().get(pos) == Some(&b'\n') {
                insert_pos = pos + 1;
            }
            content_mut.insert_str(insert_pos, &text);
        }
    }

    buffer.modified = true;
}

/// Ctrl+K Ctrl+D: Move selected occurrence to cursor position (skip current)
pub fn ctrl_k_ctrl_d_move_occurrence(
    buffer: &mut TextBuffer,
    multi_select: &MultiSelectState,
) {
    if !multi_select.active || multi_select.cursors.len() < 2 {
        return;
    }

    if let Some(first_pos) = multi_select.cursors.first() {
        if let Some(selection_text) = &multi_select.selection_text {
            // Read content first
            let content = buffer.content();
            let cursors = buffer.cursors().to_vec();

            let first_end = std::cmp::min(first_pos.0 + selection_text.len(), content.len());
            let first_text = if first_pos.0 < content.len() {
                content[first_pos.0..first_end].to_string()
            } else {
                String::new()
            };

            if let Some(cursor) = cursors.first() {
                let cur_pos = cursor.position.0;
                let cur_end = std::cmp::min(cur_pos + selection_text.len(), content.len());
                let cur_text = if cur_pos < content.len() {
                    content[cur_pos..cur_end].to_string()
                } else {
                    String::new()
                };

                if first_text == cur_text {
                    // Move text from first position to current position
                    let content_mut = buffer.content_mut();
                    content_mut.replace_range(first_pos.0..first_end, &cur_text);
                    buffer.modified = true;
                }
            }
        }
    }
}

/// Find the word at a given position in the content
pub fn extract_word_at(content: &str, pos: usize) -> String {
    if pos >= content.len() {
        return String::new();
    }

    let bytes = content.as_bytes();
    let mut start = pos;
    let mut end = pos;

    // Expand left
    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }

    // Expand right
    while end < bytes.len() && is_word_char(bytes[end]) {
        end += 1;
    }

    if start < end && start < content.len() {
        content[start..end].to_string()
    } else {
        String::new()
    }
}

/// Check if a byte is a word character (alphanumeric or underscore)
fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Find next occurrence of word starting from position
fn find_next_occurrence(content: &str, word: &str, start: usize) -> Option<usize> {
    if word.is_empty() || start >= content.len() {
        return None;
    }

    let mut search_start = start;

    while search_start <= content.len() - word.len() {
        let end = search_start + word.len();
        if content[search_start..end] == *word {
            return Some(search_start);
        }
        search_start += 1;
    }

    None
}

/// Get the byte offset of the start of the line containing pos
fn get_line_start(content: &str, pos: usize) -> usize {
    if pos >= content.len() {
        return pos;
    }
    content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Get the byte offset of the end of the line containing pos
fn get_line_end(content: &str, pos: usize) -> usize {
    if pos >= content.len() {
        return pos;
    }
    match content[pos..].find('\n') {
        Some(idx) => pos + idx,
        None => content.len(),
    }
}

/// Add cursors to all occurrences of the word under cursor (Ctrl+Shift+D in some configs)
pub fn add_cursors_to_all_occurrences(
    buffer: &mut TextBuffer,
    find_word: &str,
) {
    if find_word.is_empty() {
        return;
    }

    let content = buffer.content();
    let cursors = buffer.cursors().to_vec();
    let mut new_cursors = Vec::new();

    for cursor in &cursors {
        let pos = cursor.position.0;
        let end = std::cmp::min(pos + find_word.len(), content.len());

        if pos < content.len() && content[pos..end] == *find_word {
            // Find all occurrences
            let mut positions = Vec::new();
            let mut search_start = 0;

            while search_start <= content.len() - find_word.len() {
                if content[search_start..search_start + find_word.len()] == *find_word {
                    positions.push(Position(search_start));
                }
                search_start += 1;
            }

            // Only add cursors for occurrences not already covered
            for pos in positions {
                let already_exists = new_cursors.iter().any(|existing: &Cursor| {
                    existing.position.0 == pos.0
                });
                if !already_exists {
                    new_cursors.push(Cursor::new(pos));
                }
            }
        }
    }

    if !new_cursors.is_empty() {
        buffer.set_cursors(new_cursors);
    }
}
