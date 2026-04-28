//! Error types for `editor-core` operations.

/// The error type returned by all fallible operations in `editor-core`.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    /// An I/O error occurred while reading or writing a file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file contained bytes that are not valid UTF-8.
    #[error("file is not valid UTF-8: {0}")]
    NotUtf8(String),
}
