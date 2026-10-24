/// A named place in a document (function, class, heading…) for Goto Symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Zero-based line.
    pub line: usize,
    pub name: String,
}
