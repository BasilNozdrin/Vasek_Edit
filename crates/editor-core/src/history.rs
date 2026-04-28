//! Undo/redo history for the editor.
//!
//! Consecutive single non-newline character inserts are coalesced into one
//! undo unit (flushed on cursor movement, newline, or 500 ms idle).
//! The undo stack is bounded to `capacity` entries (default 1 000).

use std::collections::VecDeque;

use crate::{Cursor, PieceTable};

// ── EditOp ────────────────────────────────────────────────────────────────────

/// A single undoable/redoable edit.
pub struct EditOp {
    pub(crate) kind: OpKind,
    /// Cursor position before this op was applied; restored on undo.
    pub(crate) cursor_before: Cursor,
    /// Cursor position after this op was applied; restored on redo.
    pub(crate) cursor_after: Cursor,
}

pub(crate) enum OpKind {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
}

// ── History ───────────────────────────────────────────────────────────────────

struct PendingInsert {
    at: usize,
    text: String,
    cursor_before: Cursor,
    cursor_after: Cursor,
}

/// Bounded undo/redo history with single-char coalescing.
pub struct History {
    undo_stack: VecDeque<EditOp>,
    redo_stack: Vec<EditOp>,
    capacity: usize,
    pending: Option<PendingInsert>,
}

impl History {
    /// Create a history with the given maximum undo depth.
    pub fn new(capacity: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            capacity,
            pending: None,
        }
    }

    /// Record a single non-newline character insert, coalescing when consecutive.
    ///
    /// `cursor_before`/`cursor_after` are the cursor states immediately before
    /// and after the character was written to the buffer.
    pub fn push_char(&mut self, at: usize, ch: char, cursor_before: Cursor, cursor_after: Cursor) {
        if let Some(p) = &mut self.pending {
            if at == p.at + p.text.len() {
                p.text.push(ch);
                p.cursor_after = cursor_after;
                return;
            }
        }
        // Not consecutive — flush whatever was pending and start fresh.
        self.flush_pending_inner();
        self.pending = Some(PendingInsert {
            at,
            text: ch.to_string(),
            cursor_before,
            cursor_after,
        });
    }

    /// Record a multi-char or special (newline/tab) insert immediately.
    pub fn push_insert(
        &mut self,
        at: usize,
        text: String,
        cursor_before: Cursor,
        cursor_after: Cursor,
    ) {
        self.flush_pending_inner();
        self.commit(EditOp {
            kind: OpKind::Insert { at, text },
            cursor_before,
            cursor_after,
        });
    }

    /// Record a delete immediately.
    pub fn push_delete(
        &mut self,
        at: usize,
        text: String,
        cursor_before: Cursor,
        cursor_after: Cursor,
    ) {
        self.flush_pending_inner();
        self.commit(EditOp {
            kind: OpKind::Delete { at, text },
            cursor_before,
            cursor_after,
        });
    }

    /// Flush any pending coalesced insert to the undo stack.
    ///
    /// Call this on cursor movement, mode change, or idle timeout.
    pub fn flush_pending(&mut self) {
        self.flush_pending_inner();
    }

    /// `true` if there is anything on the undo stack (including pending).
    pub fn can_undo(&self) -> bool {
        self.pending.is_some() || !self.undo_stack.is_empty()
    }

    /// `true` if there is anything on the redo stack.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Undo the most recent op. Applies the inverse to `buf` and returns the
    /// cursor to restore, or `None` if the stack was empty.
    pub fn undo(&mut self, buf: &mut PieceTable) -> Option<Cursor> {
        self.flush_pending_inner();
        let op = self.undo_stack.pop_back()?;
        match &op.kind {
            OpKind::Insert { at, text } => buf.delete(*at..*at + text.len()),
            OpKind::Delete { at, text } => buf.insert(*at, text),
        }
        let cursor = op.cursor_before;
        self.redo_stack.push(op);
        Some(cursor)
    }

    /// Redo the most recently undone op. Re-applies it to `buf` and returns
    /// the cursor to restore, or `None` if the redo stack was empty.
    pub fn redo(&mut self, buf: &mut PieceTable) -> Option<Cursor> {
        let op = self.redo_stack.pop()?;
        match &op.kind {
            OpKind::Insert { at, text } => buf.insert(*at, text),
            OpKind::Delete { at, text } => buf.delete(*at..*at + text.len()),
        }
        let cursor = op.cursor_after;
        self.undo_stack.push_back(op);
        Some(cursor)
    }

    // ── private ───────────────────────────────────────────────────────────────

    fn flush_pending_inner(&mut self) {
        if let Some(p) = self.pending.take() {
            self.commit(EditOp {
                kind: OpKind::Insert {
                    at: p.at,
                    text: p.text,
                },
                cursor_before: p.cursor_before,
                cursor_after: p.cursor_after,
            });
        }
    }

    fn commit(&mut self, op: EditOp) {
        self.redo_stack.clear();
        if self.undo_stack.len() == self.capacity {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(op);
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(s: &str) -> PieceTable {
        PieceTable::from(s)
    }

    fn cur(line: usize, col: usize) -> Cursor {
        Cursor { line, col }
    }

    #[test]
    fn undo_single_insert() {
        let mut buf = pt("hello");
        let mut h = History::new(100);
        // Insert " world" at offset 5
        let cb = cur(0, 5);
        buf.insert(5, " world");
        let ca = cur(0, 11);
        h.push_insert(5, " world".to_owned(), cb, ca);

        let restored = h.undo(&mut buf).unwrap();
        assert_eq!(buf.to_string(), "hello");
        assert_eq!(restored, cb);
    }

    #[test]
    fn undo_single_delete() {
        let mut buf = pt("hello world");
        let mut h = History::new(100);
        let cb = cur(0, 5);
        // Delete " world" (6 bytes at offset 5)
        h.push_delete(5, " world".to_owned(), cb, cur(0, 5));
        buf.delete(5..11);

        let restored = h.undo(&mut buf).unwrap();
        assert_eq!(buf.to_string(), "hello world");
        assert_eq!(restored, cb);
    }

    #[test]
    fn redo_after_undo() {
        let mut buf = pt("ab");
        let mut h = History::new(100);
        let cb = cur(0, 2);
        buf.insert(2, "c");
        let ca = cur(0, 3);
        h.push_insert(2, "c".to_owned(), cb, ca);

        h.undo(&mut buf).unwrap();
        assert_eq!(buf.to_string(), "ab");

        let restored = h.redo(&mut buf).unwrap();
        assert_eq!(buf.to_string(), "abc");
        assert_eq!(restored, ca);
    }

    #[test]
    fn coalesce_single_chars() {
        let mut buf = pt("");
        let mut h = History::new(100);
        for (i, ch) in "hello".chars().enumerate() {
            let cb = cur(0, i);
            buf.insert(i, &ch.to_string());
            h.push_char(i, ch, cb, cur(0, i + 1));
        }
        h.flush_pending();
        assert_eq!(h.undo_stack.len(), 1);

        h.undo(&mut buf).unwrap();
        assert_eq!(buf.to_string(), "");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut buf = pt("a");
        let mut h = History::new(100);
        buf.insert(1, "b");
        h.push_insert(1, "b".to_owned(), cur(0, 1), cur(0, 2));
        h.undo(&mut buf).unwrap();
        assert!(h.can_redo());
        // New insert should clear redo
        buf.insert(1, "c");
        h.push_insert(1, "c".to_owned(), cur(0, 1), cur(0, 2));
        assert!(!h.can_redo());
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut buf = pt("");
        let mut h = History::new(3);
        for i in 0..5usize {
            let s = i.to_string();
            buf.insert(buf.len(), &s);
            h.push_insert(i, s.clone(), cur(0, i), cur(0, i + 1));
        }
        // Only last 3 ops survive
        assert_eq!(h.undo_stack.len(), 3);
    }

    #[test]
    fn undo_empty_returns_none() {
        let mut buf = pt("x");
        let mut h = History::new(10);
        assert!(h.undo(&mut buf).is_none());
    }
}
