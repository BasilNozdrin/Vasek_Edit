//! Document: PieceTable buffer with cursor, dirty flag, and file I/O.
//!
//! All edits go through `Document`; callers never touch `PieceTable` directly.
//! File I/O detects and preserves the original line-ending style (`\r\n` vs
//! `\n`) and strips a UTF-8 BOM on load.

use std::borrow::Cow;
use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use crate::{Cursor, EditorError, History, PieceTable};

/// Line-ending convention detected on load and restored on save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

/// A text document: piece-table buffer + cursor + file metadata + undo history.
pub struct Document {
    path: PathBuf,
    buf: PieceTable,
    /// Current cursor position.
    pub cursor: Cursor,
    dirty: bool,
    line_ending: LineEnding,
    history: History,
}

impl Document {
    /// Load a file from disk.
    ///
    /// Strips a UTF-8 BOM if present. Detects `\r\n` vs `\n` from the first
    /// line ending found (defaults to `\n` if none).
    pub fn open(path: &Path) -> Result<Self, EditorError> {
        let raw = fs::read(path)?;
        let content = if raw.starts_with(b"\xEF\xBB\xBF") {
            &raw[3..]
        } else {
            &raw[..]
        };
        let text = std::str::from_utf8(content).map_err(|e| EditorError::NotUtf8(e.to_string()))?;

        let line_ending = if text.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };

        let normalised: Cow<str> = if line_ending == LineEnding::CrLf {
            Cow::Owned(text.replace("\r\n", "\n"))
        } else {
            Cow::Borrowed(text)
        };

        Ok(Self {
            path: path.to_owned(),
            buf: PieceTable::from(normalised.as_ref()),
            cursor: Cursor::new(),
            dirty: false,
            line_ending,
            history: History::new(1000),
        })
    }

    /// Save to the original path atomically (write to `.tmp`, then rename).
    ///
    /// Restores the detected line-ending convention.
    pub fn save(&mut self) -> Result<(), EditorError> {
        let content = self.buf.to_string();
        let out: Cow<str> = if self.line_ending == LineEnding::CrLf {
            Cow::Owned(content.replace('\n', "\r\n"))
        } else {
            Cow::Borrowed(content.as_str())
        };

        let tmp = self.path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(out.as_bytes())?;
            f.flush()?;
        }
        fs::rename(&tmp, &self.path)?;
        self.dirty = false;
        Ok(())
    }

    /// `true` if there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Path this document was loaded from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The line-ending convention detected on load.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.buf.line_count()
    }

    /// Content of line `line` (0-indexed), without the trailing `\n`.
    pub fn line_at(&self, line: usize) -> Option<Cow<'_, str>> {
        self.buf.line_at(line)
    }

    // ── editing ──────────────────────────────────────────────────────────────

    /// Insert `text` at the cursor and advance the cursor past it.
    pub fn insert_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let at = self.cursor_byte_offset();
        let cursor_before = self.cursor;
        self.buf.insert(at, text);
        self.dirty = true;
        self.advance_cursor_by(text);
        let cursor_after = self.cursor;
        // Coalesce single non-newline chars; commit everything else immediately.
        let mut chars = text.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if ch != '\n' {
                self.history.push_char(at, ch, cursor_before, cursor_after);
                return;
            }
        }
        self.history
            .push_insert(at, text.to_owned(), cursor_before, cursor_after);
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        let off = self.cursor_byte_offset();
        if off == 0 {
            return;
        }
        let step = self.prev_char_len(off);
        let deleted = self.buf.slice(off - step..off).into_owned();
        let cursor_before = self.cursor;
        self.buf.delete(off - step..off);
        self.dirty = true;
        self.move_cursor_back(step);
        self.history
            .push_delete(off - step, deleted, cursor_before, self.cursor);
    }

    /// Delete the character at the cursor (delete key).
    pub fn delete_forward(&mut self) {
        let off = self.cursor_byte_offset();
        if off >= self.buf.len() {
            return;
        }
        let step = self.next_char_len(off);
        let deleted = self.buf.slice(off..off + step).into_owned();
        let cursor_before = self.cursor;
        self.buf.delete(off..off + step);
        self.dirty = true;
        self.clamp_cursor();
        self.history
            .push_delete(off, deleted, cursor_before, self.cursor);
    }

    /// Undo the most recent edit. Returns `true` if anything was undone.
    pub fn undo(&mut self) -> bool {
        if let Some(cursor) = self.history.undo(&mut self.buf) {
            self.cursor = cursor;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Redo the most recently undone edit. Returns `true` if anything was redone.
    pub fn redo(&mut self) -> bool {
        if let Some(cursor) = self.history.redo(&mut self.buf) {
            self.cursor = cursor;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Flush any pending coalesced insert to the history stack.
    ///
    /// Call on cursor movement, mode switch, or idle timeout.
    pub fn flush_history(&mut self) {
        self.history.flush_pending();
    }

    // ── cursor movement ──────────────────────────────────────────────────────

    /// Move cursor up one line, clamping the column.
    pub fn move_up(&mut self) {
        self.history.flush_pending();
        if self.cursor.line == 0 {
            self.cursor.col = 0;
            return;
        }
        self.cursor.line -= 1;
        self.clamp_cursor();
    }

    /// Move cursor down one line, clamping the column.
    pub fn move_down(&mut self) {
        self.history.flush_pending();
        if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.clamp_cursor();
        }
    }

    /// Move cursor one char to the left, wrapping to the previous line.
    pub fn move_left(&mut self) {
        self.history.flush_pending();
        if self.cursor.col > 0 {
            let line = self.line_str(self.cursor.line);
            self.cursor.col = prev_char_boundary(&line, self.cursor.col);
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_str(self.cursor.line).len();
        }
    }

    /// Move cursor one char to the right, wrapping to the next line.
    pub fn move_right(&mut self) {
        self.history.flush_pending();
        let line = self.line_str(self.cursor.line);
        if self.cursor.col < line.len() {
            self.cursor.col = next_char_boundary(&line, self.cursor.col);
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// Move cursor to the start of the current line.
    pub fn move_home(&mut self) {
        self.history.flush_pending();
        self.cursor.col = 0;
    }

    /// Move cursor to the end of the current line.
    pub fn move_end(&mut self) {
        self.history.flush_pending();
        self.cursor.col = self.line_str(self.cursor.line).len();
    }

    /// Move cursor up by `page_height` lines.
    pub fn page_up(&mut self, page_height: usize) {
        self.history.flush_pending();
        self.cursor.line = self.cursor.line.saturating_sub(page_height);
        self.clamp_cursor();
    }

    /// Move cursor down by `page_height` lines.
    pub fn page_down(&mut self, page_height: usize) {
        self.history.flush_pending();
        let max = self.line_count().saturating_sub(1);
        self.cursor.line = (self.cursor.line + page_height).min(max);
        self.clamp_cursor();
    }

    /// Jump left past whitespace then past word chars (Ctrl+Left).
    pub fn word_left(&mut self) {
        self.history.flush_pending();
        let line = self.line_str(self.cursor.line);
        let b = line.as_bytes();
        while self.cursor.col > 0 {
            let prev = prev_char_boundary(&line, self.cursor.col);
            if b[prev] == b' ' {
                self.cursor.col = prev;
            } else {
                break;
            }
        }
        while self.cursor.col > 0 {
            let prev = prev_char_boundary(&line, self.cursor.col);
            if b[prev] != b' ' {
                self.cursor.col = prev;
            } else {
                break;
            }
        }
    }

    /// Jump right past word chars then past whitespace (Ctrl+Right).
    pub fn word_right(&mut self) {
        self.history.flush_pending();
        let line = self.line_str(self.cursor.line);
        let b = line.as_bytes();
        while self.cursor.col < line.len() {
            if b[self.cursor.col] != b' ' {
                self.cursor.col = next_char_boundary(&line, self.cursor.col);
            } else {
                break;
            }
        }
        while self.cursor.col < line.len() {
            if b[self.cursor.col] == b' ' {
                self.cursor.col = next_char_boundary(&line, self.cursor.col);
            } else {
                break;
            }
        }
        if self.cursor.col >= line.len() && self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    // ── private helpers ──────────────────────────────────────────────────────

    /// Flat byte offset of the cursor in the piece table.
    pub fn cursor_byte_offset(&self) -> usize {
        let mut off = 0usize;
        for l in 0..self.cursor.line {
            off += self.buf.line_at(l).map_or(0, |s| s.len()) + 1;
        }
        off + self.cursor.col
    }

    fn line_str(&self, line: usize) -> String {
        self.buf
            .line_at(line)
            .map(|c| c.into_owned())
            .unwrap_or_default()
    }

    fn clamp_cursor(&mut self) {
        let line = self.line_str(self.cursor.line);
        if self.cursor.col > line.len() {
            self.cursor.col = line.len();
        }
        while self.cursor.col > 0 && !line.is_char_boundary(self.cursor.col) {
            self.cursor.col -= 1;
        }
    }

    fn advance_cursor_by(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.cursor.line += 1;
                self.cursor.col = 0;
            } else {
                self.cursor.col += ch.len_utf8();
            }
        }
    }

    fn move_cursor_back(&mut self, step: usize) {
        if self.cursor.col >= step {
            self.cursor.col -= step;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_str(self.cursor.line).len();
        } else {
            self.cursor.col = 0;
        }
    }

    fn prev_char_len(&self, byte_off: usize) -> usize {
        let full = self.buf.slice(0..byte_off);
        let s = full.as_ref();
        // Length of the last character in `s`.
        s.char_indices()
            .next_back()
            .map_or(1, |(i, _)| s.len() - i)
            .min(byte_off)
    }

    fn next_char_len(&self, byte_off: usize) -> usize {
        let end = (byte_off + 4).min(self.buf.len());
        let chunk = self.buf.slice(byte_off..end);
        chunk.chars().next().map_or(1, char::len_utf8)
    }
}

fn prev_char_boundary(s: &str, mut pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    pos -= 1;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn next_char_boundary(s: &str, mut pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    pos += 1;
    while pos < s.len() && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vasek_doc_{tag}_{}.txt", std::process::id()))
    }

    #[test]
    fn open_lf_lines() {
        let p = temp_path("open_lf");
        fs::write(&p, "hello\nworld\n").unwrap();
        let doc = Document::open(&p).unwrap();
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.line_at(0).unwrap().as_ref(), "hello");
        assert_eq!(doc.line_at(1).unwrap().as_ref(), "world");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn save_byte_identical_lf() {
        let p = temp_path("lf");
        let orig = "hello\nworld\n";
        fs::write(&p, orig).unwrap();
        let mut doc = Document::open(&p).unwrap();
        assert!(!doc.is_dirty());
        doc.save().unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), orig);
        fs::remove_file(&p).ok();
    }

    #[test]
    fn save_byte_identical_crlf() {
        let p = temp_path("crlf");
        let orig = "hello\r\nworld\r\n";
        fs::write(&p, orig).unwrap();
        let mut doc = Document::open(&p).unwrap();
        doc.save().unwrap();
        assert_eq!(fs::read(&p).unwrap(), orig.as_bytes());
        fs::remove_file(&p).ok();
    }

    #[test]
    fn bom_stripped() {
        let p = temp_path("bom");
        fs::write(&p, b"\xEF\xBB\xBFhello").unwrap();
        let doc = Document::open(&p).unwrap();
        assert_eq!(doc.line_at(0).unwrap().as_ref(), "hello");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn insert_sets_dirty_and_moves_cursor() {
        let p = temp_path("insert_dirty");
        fs::write(&p, "").unwrap();
        let mut doc = Document::open(&p).unwrap();
        assert!(!doc.is_dirty());
        doc.insert_at_cursor("ab\ncd");
        assert!(doc.is_dirty());
        assert_eq!(doc.cursor.line, 1);
        assert_eq!(doc.cursor.col, 2);
        fs::remove_file(&p).ok();
    }

    #[test]
    fn backspace_at_origin_is_noop() {
        let p = temp_path("backspace");
        fs::write(&p, "hi").unwrap();
        let mut doc = Document::open(&p).unwrap();
        doc.backspace();
        assert_eq!(doc.line_at(0).unwrap().as_ref(), "hi");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn save_clears_dirty() {
        let p = temp_path("dirty");
        fs::write(&p, "x").unwrap();
        let mut doc = Document::open(&p).unwrap();
        doc.insert_at_cursor("y");
        assert!(doc.is_dirty());
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        fs::remove_file(&p).ok();
    }

    // ── undo/redo ─────────────────────────────────────────────────────────────

    fn make_doc_str(tag: &str, content: &str) -> (Document, PathBuf) {
        let p = temp_path(tag);
        fs::write(&p, content).unwrap();
        let doc = Document::open(&p).unwrap();
        (doc, p)
    }

    #[test]
    fn undo_single_insert_restores_content() {
        let (mut doc, p) = make_doc_str("undo_ins", "hello");
        doc.insert_at_cursor("!");
        assert_eq!(doc.buf.to_string(), "!hello");
        doc.undo();
        assert_eq!(doc.buf.to_string(), "hello");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn redo_reapplies_insert() {
        let (mut doc, p) = make_doc_str("redo_ins", "a");
        doc.move_end();
        doc.insert_at_cursor("b");
        doc.undo();
        assert_eq!(doc.buf.to_string(), "a");
        doc.redo();
        assert_eq!(doc.buf.to_string(), "ab");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn undo_backspace_restores_content() {
        let (mut doc, p) = make_doc_str("undo_bs", "hi");
        doc.move_end();
        doc.backspace();
        assert_eq!(doc.buf.to_string(), "h");
        doc.undo();
        assert_eq!(doc.buf.to_string(), "hi");
        fs::remove_file(&p).ok();
    }

    #[test]
    fn coalesced_chars_undo_in_one_step() {
        let (mut doc, p) = make_doc_str("coalesce", "");
        for ch in "hello".chars() {
            let mut buf = [0u8; 4];
            doc.insert_at_cursor(ch.encode_utf8(&mut buf));
        }
        // All 5 chars should undo in a single step
        doc.undo();
        assert_eq!(doc.buf.to_string(), "");
        assert!(!doc.undo()); // nothing left
        fs::remove_file(&p).ok();
    }

    proptest::proptest! {
        #[test]
        fn undo_any_sequence_restores_initial(
            initial in "[a-z \n]{0,40}",
            ops in proptest::collection::vec(proptest::bool::ANY, 0..30),
        ) {
            let p = temp_path("prop_undo");
            fs::write(&p, &initial).unwrap();
            let mut doc = Document::open(&p).unwrap();
            let original = doc.buf.to_string();

            for do_insert in &ops {
                if *do_insert {
                    doc.insert_at_cursor("x");
                } else {
                    doc.backspace();
                }
            }

            // Undo everything
            for _ in 0..ops.len() + 1 {
                doc.undo();
            }

            proptest::prop_assert_eq!(doc.buf.to_string(), original);
            fs::remove_file(&p).ok();
        }
    }
}
