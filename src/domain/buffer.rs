//! Buffer - High-level file buffer that combines text buffer, syntax, and file state

use std::sync::Arc;
use uuid::Uuid;
use crate::domain::text_buffer::TextBuffer;

/// File state tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileState {
    Fresh,           // New file, never saved
    Modified,        // Has unsaved changes
    Saved,           // In sync with disk
    Deleted,         // File deleted from disk
}

/// A sheet/buffer in the editor
#[derive(Debug, Clone)]
pub struct Buffer {
    pub id: Uuid,
    pub text: TextBuffer,
    pub state: FileState,
    pub zoom_level: f32,
    pub font_size: f32,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub show_minimap: bool,
    pub tab_size: usize,
    pub use_tabs: bool,
}

impl Buffer {
    pub fn new(name: String, file_path: Option<Arc<str>>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: TextBuffer::new(name, file_path),
            state: FileState::Fresh,
            zoom_level: 1.0,
            font_size: 14.0,
            word_wrap: false,
            show_line_numbers: true,
            show_minimap: false,
            tab_size: 4,
            use_tabs: false,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        self.text.name()
    }

    pub fn set_name(&mut self, name: String) {
        self.text = TextBuffer::new(name, self.text.file_path().map(Arc::from));
    }

    pub fn file_path(&self) -> Option<&str> {
        self.text.file_path()
    }

    pub fn is_modified(&self) -> bool {
        self.text.is_modified()
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.text.set_modified(modified);
        if modified {
            self.state = FileState::Modified;
        }
    }

    pub fn content(&self) -> &str {
        self.text.content()
    }

    pub fn content_mut(&mut self) -> &mut TextBuffer {
        &mut self.text
    }

    pub fn syntax(&self) -> &str {
        self.text.syntax()
    }

    pub fn set_syntax(&mut self, syntax: String) {
        self.text.set_syntax(syntax);
    }

    pub fn set_file_path(&mut self, path: Option<Arc<str>>) {
        self.text = TextBuffer::new(self.text.name().to_string(), path);
    }

    pub fn line_count(&self) -> usize {
        self.text.line_count()
    }

    pub fn cursor_count(&self) -> usize {
        self.text.cursors().len()
    }

    pub fn line(&self, line: usize) -> Option<&str> {
        self.text.line(line)
    }

    pub fn cursors(&self) -> &[crate::domain::text_buffer::Cursor] {
        self.text.cursors()
    }

    pub fn chars(&self) -> std::str::Chars<'_> {
        self.text.content().chars()
    }

    pub fn drain(&mut self, range: std::ops::Range<usize>) {
        self.text.delete(range)
    }

    pub fn len(&self) -> usize {
        self.text.content().len()
    }

    /// Determine syntax from file extension
    pub fn detect_syntax(&mut self) {
        if let Some(path) = self.file_path() {
            if let Some(ext) = std::path::Path::new(path).extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                self.set_syntax(match ext.as_str() {
                    "rs" => "rust".to_string(),
                    "py" => "python".to_string(),
                    "js" | "mjs" | "cjs" => "javascript".to_string(),
                    "ts" | "mts" | "cts" => "typescript".to_string(),
                    "cpp" | "cc" | "cxx" | "hpp" | "h" => "cpp".to_string(),
                    "html" | "htm" => "html".to_string(),
                    "css" | "scss" | "sass" => "css".to_string(),
                    "json" => "json".to_string(),
                    "md" => "markdown".to_string(),
                    "txt" => "text".to_string(),
                    _ => "text".to_string(),
                });
            }
        }
    }
}
