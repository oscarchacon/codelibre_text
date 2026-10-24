//! Editor - The main text editing widget with syntax highlighting, multi-cursor, etc.

use crate::domain::buffer::Buffer;
use crate::services::syntax::{SyntaxHighlighter, HighlightType, HighlightChunk};
use crate::domain::text_buffer::Position;
use eframe::egui;

/// Editor view state
#[derive(Debug)]
pub struct EditorView {
    buffer: Buffer,
    scroll_offset: egui::Vec2,
    zoom_factor: f32,
    pub vertical_scroll: f32,
    pub horizontal_scroll: f32,
    line_height: f32,
    char_width: f32,
    /// The currently typed string (committed at end of frame)
    committed_text: String,
    /// IME composition text
    ime_text: Option<String>,
}

impl EditorView {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            scroll_offset: egui::Vec2::default(),
            zoom_factor: 1.0,
            vertical_scroll: 0.0,
            horizontal_scroll: 0.0,
            line_height: 18.0,
            char_width: 9.0,
            committed_text: String::new(),
            ime_text: None,
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    pub fn zoom_in(&mut self) {
        self.zoom_factor = (self.zoom_factor + 0.1).min(2.0);
        self.line_height = 18.0 * self.zoom_factor;
        self.char_width = 9.0 * self.zoom_factor;
    }

    pub fn zoom_out(&mut self) {
        self.zoom_factor = (self.zoom_factor - 0.1).max(0.5);
        self.line_height = 18.0 * self.zoom_factor;
        self.char_width = 9.0 * self.zoom_factor;
    }

    pub fn reset_zoom(&mut self) {
        self.zoom_factor = 1.0;
        self.line_height = 18.0;
        self.char_width = 9.0;
    }

    /// Handle keyboard input
    pub fn handle_input(&mut self, key: egui::Key, modifiers: egui::Modifiers, character: Option<char>) {
        let _content = self.buffer.content_mut();

        match key {
            egui::Key::Backspace => {
                self._handle_backspace();
            }
            egui::Key::Delete => {
                self._handle_delete();
            }
            egui::Key::Enter => {
                self._handle_enter();
            }
            egui::Key::Tab => {
                if modifiers.shift {
                    self._handle_unindent();
                } else {
                    self._handle_indent();
                }
            }
            egui::Key::A if modifiers.ctrl => {
                self._select_all();
            }
            egui::Key::Z if modifiers.ctrl && !modifiers.shift => {
                // Undo - handled by the app
            }
            egui::Key::Y if modifiers.ctrl => {
                // Redo - handled by the app
            }
            _ => {
                // Regular character input
                if let Some(ch) = character {
                    self.committed_text.push(ch);
                }
            }
        }
    }

    /// Commit typed text to the buffer
    pub fn commit_text(&mut self) {
        if self.committed_text.is_empty() {
            return;
        }

        let content = self.buffer.content_mut();
        let cursors = content.cursors();

        if let Some(cursor) = cursors.first() {
            content.insert(cursor.position, &self.committed_text);
        }

        self.committed_text.clear();
    }

    /// Draw the editor content
    pub fn ui(&mut self, ctx: &egui::Context, _frame_bg: egui::Color32) {
        let content = self.buffer.content();
        let line_count = self.buffer.line_count();

        // Calculate visible lines based on scroll
        let available_height = ctx.available_rect().height();
        let viewport_lines = (available_height / self.line_height) as usize;
        let start_line = self.vertical_scroll as usize;
        let end_line = std::cmp::min(start_line + viewport_lines + 1, line_count);

        // Get syntax highlighter for this buffer's syntax
        let mut highlighter = SyntaxHighlighter::new(self.buffer.syntax())
            .unwrap_or_else(|_| SyntaxHighlighter::new("text").unwrap());
        let highlights = highlighter.highlight(content);

        for line_idx in start_line..end_line {
            let line_num = line_idx + 1;
            let y_offset = (line_idx as f32 - self.vertical_scroll) * self.line_height;

            if y_offset < -self.line_height || y_offset > available_height {
                continue;
            }

            let line_text = self.buffer.line(line_idx).unwrap_or("");

            // Draw line number
            let line_fg = if line_num == 1 {
                egui::Color32::from_rgb(200, 200, 220)
            } else {
                egui::Color32::from_rgb(100, 100, 120)
            };

            ctx.debug_painter().text(
                egui::Pos2::new(8.0, y_offset + self.line_height - 4.0),
                egui::Align2::LEFT_TOP,
                format!("{} ", line_num),
                egui::FontId::monospace(self.line_height),
                line_fg,
            );

            // Draw syntax highlighted text
            self._draw_line(ctx, line_text, line_idx, line_num, y_offset, &highlights);
        }

        // Draw cursor
        self._draw_cursor(ctx);
    }

    fn _draw_line(
        &self,
        ctx: &egui::Context,
        text: &str,
        line_idx: usize,
        line_num: usize,
        y_offset: f32,
        highlights: &[HighlightChunk],
    ) {
        let text_fg = if line_num == 1 {
            egui::Color32::from_rgb(220, 220, 220)
        } else {
            egui::Color32::from_rgb(200, 200, 200)
        };

        let base_x = 100.0; // After line numbers

        // Find highlights for this line
        let line_start = if line_idx > 0 {
            self.buffer.content().lines().take(line_idx).map(|l| l.len() + 1).sum::<usize>()
        } else {
            0
        };
        let line_end = line_start + text.len();

        for chunk in highlights.iter().filter(|c| c.start < line_end && c.end >= line_start) {
            let chunk_start = std::cmp::max(chunk.start, line_start) - line_start;
            let chunk_end = std::cmp::min(chunk.end, line_end) - line_start;

            if chunk_start < text.len() && chunk_end <= text.len() {
                let chunk_text = &text[chunk_start..chunk_end];

                let color = match chunk.highlight_type {
                    HighlightType::Keyword => egui::Color32::from_rgb(255, 120, 200),
                    HighlightType::Type => egui::Color32::from_rgb(190, 210, 255),
                    HighlightType::Function => egui::Color32::from_rgb(130, 170, 255),
                    HighlightType::String => egui::Color32::from_rgb(170, 200, 120),
                    HighlightType::Number => egui::Color32::from_rgb(240, 180, 100),
                    HighlightType::Comment => egui::Color32::from_rgb(80, 110, 80),
                    HighlightType::Variable => text_fg,
                    HighlightType::Constant => egui::Color32::from_rgb(100, 180, 255),
                    HighlightType::Parameter => egui::Color32::from_rgb(255, 200, 130),
                    HighlightType::Operator => egui::Color32::from_rgb(150, 180, 220),
                    HighlightType::Punctuation => egui::Color32::from_rgb(120, 140, 180),
                    HighlightType::Tag => egui::Color32::from_rgb(255, 120, 120),
                    HighlightType::Attribute => egui::Color32::from_rgb(180, 160, 255),
                    HighlightType::Value => egui::Color32::from_rgb(170, 220, 120),
                };

                ctx.debug_painter().text(
                    egui::Pos2::new(base_x + chunk_start as f32 * self.char_width, y_offset + self.line_height - 4.0),
                    egui::Align2::LEFT_TOP,
                    chunk_text,
                    egui::FontId::monospace(self.line_height),
                    color,
                );
            }
        }

        // Draw non-highlighted text
        if text.is_empty() {
            return;
        }

        // Check if we have any highlights that cover this line
        let covered: Vec<(usize, usize)> = highlights.iter()
            .filter(|c| c.start < line_end && c.end >= line_start)
            .map(|c| {
                let s = std::cmp::max(c.start, line_start) - line_start;
                let e = std::cmp::min(c.end, line_end) - line_start;
                (s, e)
            })
            .collect();

        if covered.is_empty() {
            // No highlights, draw entire line normally
            ctx.debug_painter().text(
                egui::Pos2::new(base_x, y_offset + self.line_height - 4.0),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::monospace(self.line_height),
                text_fg,
            );
        }
    }

    fn _draw_cursor(&self, ctx: &egui::Context) {
        let content = self.buffer.content();
        if let Some(cursor) = self.buffer.cursors().first() {
            // Calculate cursor position on screen
            let pos = self._position_to_screen(cursor.position);
            let cursor_x = pos.x + 2.0;
            let cursor_y = (pos.y - self.vertical_scroll) * self.line_height;
            let avail_height = ctx.available_rect().height();

            if cursor_y >= 0.0 && cursor_y < avail_height {
                ctx.debug_painter().rect_stroke(
                    egui::Rect::from_min_size(
                        egui::Pos2::new(cursor_x, cursor_y),
                        egui::Vec2::new(2.0, self.line_height),
                    ),
                    0.0_f32,
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(220, 220, 220)),
                    egui::epaint::StrokeKind::Inside,
                );
            }
        }
    }

    fn _position_to_screen(&self, pos: Position) -> egui::Pos2 {
        let content = self.buffer.content();
        let text = &content[pos.0.min(content.len())..];

        let mut char_idx = 0;
        let mut line = 0;
        let mut _col = 0;

        for c in text.chars() {
            if c == '\n' {
                line += 1;
                _col = 0;
            } else {
                _col += 1;
            }
            char_idx += c.len_utf8();
            if char_idx >= 1 {
                break;
            }
        }

        // Get the position within the current line
        let byte_pos = pos.0;
        let content_str = content;
        let line_start = if byte_pos > 0 && byte_pos <= content_str.len() {
            content_str[..byte_pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
        } else {
            0
        };
        let current_line_text = &content_str[line_start..byte_pos.min(content_str.len())];

        let x = 100.0 + current_line_text.chars().count() as f32 * self.char_width;
        let y = line as f32;

        egui::Pos2::new(x, y)
    }

    fn _handle_backspace(&mut self) {
        let content = self.buffer.content();
        if let Some(cursor) = self.buffer.cursors().first() {
            if cursor.position.0 > 0 {
                let pos = cursor.position.0 - 1;
                let c = self.buffer.chars().nth(pos).map(|c| c.len_utf8()).unwrap_or(1);
                self.buffer.drain(pos..pos + c);
            }
        }
    }

    fn _handle_delete(&mut self) {
        let content = self.buffer.content();
        if let Some(cursor) = self.buffer.cursors().first() {
            let pos = cursor.position.0;
            if pos < self.buffer.len() {
                let c = self.buffer.chars().nth(pos).map(|c| c.len_utf8()).unwrap_or(1);
                self.buffer.drain(pos..pos + c);
            }
        }
    }

    fn _handle_enter(&mut self) {
        self.committed_text.push('\n');
    }

    fn _handle_indent(&mut self) {
        let spaces = "    ";
        self.committed_text.push_str(spaces);
    }

    fn _handle_unindent(&mut self) {
        // Remove leading whitespace
    }

    pub fn _select_all(&mut self) {
        // Select all text
    }
}
