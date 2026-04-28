//! Cursor position within a document.
//!
//! A cursor is a (line, col) pair where `col` is a byte offset within the
//! line (always landing on a char boundary). Both values are 0-indexed.

/// Cursor position in (line, col) coordinates.
///
/// `col` is a byte offset from the start of the line, always on a UTF-8
/// char boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
}

impl Cursor {
    /// Create a cursor at the origin.
    pub fn new() -> Self {
        Self::default()
    }
}
