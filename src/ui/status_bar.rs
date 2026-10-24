//! Status bar - Bottom bar showing cursor position, encoding, syntax, etc.

use crate::domain::buffer::Buffer;

#[derive(Debug, Clone)]
pub struct StatusBar {
    pub visible: bool,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub selection_count: usize,
    pub file_name: String,
    pub syntax: String,
    pub encoding: String,
    pub line_ending: LineEnding,
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
    pub notification: Option<String>,
    pub notification_timeout: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    CRLF,
    LF,
}

impl LineEnding {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CRLF => "CRLF",
            Self::LF => "LF",
        }
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            visible: true,
            cursor_line: 0,
            cursor_column: 0,
            selection_count: 0,
            file_name: "untitled".to_string(),
            syntax: "Plain Text".to_string(),
            encoding: "UTF-8".to_string(),
            line_ending: LineEnding::CRLF,
            git_branch: None,
            git_status: None,
            notification: None,
            notification_timeout: 0.0,
        }
    }

    pub fn update_from_buffer(&mut self, buffer: &Buffer) {
        self.file_name = buffer.name().to_string();
        self.syntax = buffer.syntax().to_string();

        let content = buffer.content();
        if !content.is_empty() {
            self.line_ending = if content.contains("\r\n") {
                LineEnding::CRLF
            } else {
                LineEnding::LF
            };
        }

        // Update cursor position
        if let Some(cursors) = self.get_active_cursor(buffer) {
            self.cursor_line = cursors.0;
            self.cursor_column = cursors.1;
        }

        // Update selection count
        self.selection_count = buffer.cursor_count();
    }

    fn get_active_cursor(&self, buffer: &Buffer) -> Option<(usize, usize)> {
        if let Some(path) = buffer.file_path() {
            let content = buffer.content();
            let cursor_pos = 0; // Simplified - would track actual cursor
            let line = content[..cursor_pos].chars().filter(|&c| c == '\n').count() + 1;
            let col = content[..cursor_pos].lines().last()
                .map(|l| l.chars().count())
                .unwrap_or(0);
            Some((line, col + 1))
        } else {
            Some((1, 1))
        }
    }

    pub fn set_notification(&mut self, message: String, timeout: f64) {
        self.notification = Some(message);
        self.notification_timeout = timeout;
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    pub fn set_git_branch(&mut self, branch: Option<String>) {
        self.git_branch = branch;
    }

    pub fn set_git_status(&mut self, status: Option<String>) {
        self.git_status = status;
    }

    pub fn format_status(&self) -> String {
        let mut parts = Vec::new();

        // File info
        parts.push(format!(" {}", self.file_name));

        // Cursor position
        parts.push(format!("Ln {}, Col {}", self.cursor_line, self.cursor_column));

        // Selection info
        if self.selection_count > 0 {
            parts.push(format!(" ({} selected)", self.selection_count));
        }

        // Git info
        if let Some(branch) = &self.git_branch {
            parts.push(format!(" ({})", branch));
        }

        // Syntax
        parts.push(format!(" ({})", self.syntax));

        // Encoding
        parts.push(format!(" {}", self.encoding));

        // Line ending
        parts.push(format! (" {}", self.line_ending.as_str()));

        parts.join(" | ")
    }
}
