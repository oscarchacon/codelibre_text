/// A selection inside a document, as char indices. `anchor == head` is a plain caret.
///
/// `anchor` stays put while extending a selection; `head` is the end that moves (where the caret is).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(position: usize) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The lower end of the selection.
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// The upper end of the selection.
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// The same selection limited to a text of `len` chars.
    pub fn clamped(&self, len: usize) -> Self {
        Self::new(self.anchor.min(len), self.head.min(len))
    }
}
