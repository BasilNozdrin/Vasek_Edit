//! Read-only text buffer backed by a `Vec<String>`.
//!
//! Phase 2 replaces these internals with a PieceTable while keeping the
//! public API stable so `editor-tui` needs no changes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::EditorError;

/// A read-only view of a UTF-8 text file loaded into memory.
///
/// Lines are stored without their trailing newline characters.
/// Both `\n` and `\r\n` line endings are accepted on load.
///
/// # Example
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use editor_core::Buffer;
/// use std::path::Path;
///
/// let buf = Buffer::from_file(Path::new("PLAN.md"))?;
/// println!("{} lines", buf.line_count());
/// # Ok(())
/// # }
/// ```
pub struct Buffer {
    path: PathBuf,
    lines: Vec<String>,
}

impl Buffer {
    /// Load a UTF-8 text file from `path` into a read-only buffer.
    pub fn from_file(path: &Path) -> Result<Self, EditorError> {
        let content = fs::read_to_string(path)?;
        let lines = content.lines().map(str::to_owned).collect();
        Ok(Self {
            path: path.to_owned(),
            lines,
        })
    }

    /// Returns every line in the buffer, without trailing newline characters.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns the number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns the path from which this buffer was loaded.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vasek_edit_{name}"));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn load_simple_file() {
        let path = write_temp("buf_simple", "hello\nworld\n");
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[0], "hello");
        assert_eq!(buf.lines()[1], "world");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_crlf_file() {
        let path = write_temp("buf_crlf", "hello\r\nworld\r\n");
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[0], "hello");
        assert_eq!(buf.lines()[1], "world");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_no_trailing_newline() {
        let path = write_temp("buf_no_trail", "a\nb");
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.lines()[1], "b");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_empty_file() {
        let path = write_temp("buf_empty", "");
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.line_count(), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_returns_error() {
        let result = Buffer::from_file(Path::new("c:/nonexistent/path/that/cannot/exist/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn path_is_preserved() {
        let path = write_temp("buf_path", "test");
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.path(), path);
        std::fs::remove_file(&path).ok();
    }
}
