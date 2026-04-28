//! Piece table text buffer.
//!
//! Two source buffers (`original` read-only, `add` append-only) and a sequence
//! of [`Piece`]s that together describe the current content. Insert splits a
//! piece; delete removes or trims pieces. Both operations are O(piece count).
//!
//! A sorted `newline_offsets` vec tracks `\n` positions for O(log n) line
//! queries. A Fenwick tree over piece lengths would give O(log n) lookup;
//! the current linear scan of pieces is O(piece count) which is already
//! independent of buffer byte size.

use std::borrow::Cow;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Original,
    Add,
}

#[derive(Clone, Debug)]
struct Piece {
    source: Source,
    /// Byte offset into the source buffer.
    offset: usize,
    /// Byte length of this piece.
    length: usize,
}

/// A piece table text buffer supporting efficient insert and delete.
///
/// # Line model
///
/// Every `\n` starts a new line. An empty buffer has 0 lines; a non-empty
/// buffer always has at least 1 line regardless of whether it ends with `\n`.
/// A trailing `\n` creates an empty final line (`line_count` = `\n` count + 1).
pub struct PieceTable {
    original: String,
    add: String,
    pieces: Vec<Piece>,
    /// Total byte length of the logical content.
    total_bytes: usize,
    /// Sorted byte positions of every `\n` in the logical content.
    newline_offsets: Vec<usize>,
}

impl PieceTable {
    /// Create an empty `PieceTable`.
    pub fn new() -> Self {
        Self {
            original: String::new(),
            add: String::new(),
            pieces: Vec::new(),
            total_bytes: 0,
            newline_offsets: Vec::new(),
        }
    }

    fn load(s: &str) -> Self {
        let mut t = Self::new();
        if !s.is_empty() {
            t.original = s.to_owned();
            t.pieces.push(Piece {
                source: Source::Original,
                offset: 0,
                length: s.len(),
            });
            t.total_bytes = s.len();
            t.newline_offsets = s
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i)
                .collect();
        }
        t
    }

    /// Total byte length of the content.
    pub fn len(&self) -> usize {
        self.total_bytes
    }

    /// Returns `true` if the buffer contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.total_bytes == 0
    }

    /// Number of lines in the buffer.
    ///
    /// An empty buffer has 0 lines. Otherwise the count equals
    /// `(number of '\n' chars) + 1`.
    pub fn line_count(&self) -> usize {
        if self.total_bytes == 0 {
            0
        } else {
            self.newline_offsets.len() + 1
        }
    }

    /// Convert a flat byte offset to `(line, col)` using the newline index.
    ///
    /// Clamps `offset` to `[0, self.len()]`. The returned `col` is a byte
    /// column within the line.
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.total_bytes);
        let line = self.newline_offsets.partition_point(|&nl| nl < offset);
        let line_start = if line == 0 {
            0
        } else {
            self.newline_offsets[line - 1] + 1
        };
        (line, offset - line_start)
    }

    /// Content of line `line` (0-indexed), without the trailing `\n`.
    ///
    /// Returns `None` if `line >= self.line_count()`.
    pub fn line_at(&self, line: usize) -> Option<Cow<'_, str>> {
        if self.total_bytes == 0 {
            return None;
        }
        let count = self.newline_offsets.len() + 1;
        if line >= count {
            return None;
        }
        let start = if line == 0 {
            0
        } else {
            self.newline_offsets[line - 1] + 1
        };
        let end = if line < self.newline_offsets.len() {
            self.newline_offsets[line]
        } else {
            self.total_bytes
        };
        Some(self.slice(start..end))
    }

    /// Bytes `range` of the logical content as a string slice.
    ///
    /// Returns `Cow::Borrowed` when the range falls within a single piece,
    /// `Cow::Owned` otherwise.
    ///
    /// # Panics
    /// Panics if `range.end > self.len()`.
    pub fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        assert!(range.end <= self.total_bytes, "slice out of bounds");
        if range.start == range.end {
            return Cow::Borrowed("");
        }
        let mut pos = 0usize;
        let mut only_chunk: Option<&str> = None;
        let mut owned = String::new();

        for piece in &self.pieces {
            let piece_end = pos + piece.length;
            if piece_end <= range.start {
                pos = piece_end;
                continue;
            }
            if pos >= range.end {
                break;
            }
            let src = self.src(piece.source);
            let lo = range.start.saturating_sub(pos);
            let hi = piece.length.min(range.end - pos);
            let chunk = &src[piece.offset + lo..piece.offset + hi];

            if only_chunk.is_none() && owned.is_empty() {
                only_chunk = Some(chunk);
            } else {
                if let Some(first) = only_chunk.take() {
                    owned.push_str(first);
                }
                owned.push_str(chunk);
            }
            pos = piece_end;
        }

        if let Some(only) = only_chunk {
            Cow::Borrowed(only)
        } else {
            Cow::Owned(owned)
        }
    }

    /// Insert `text` at byte offset `at`.
    ///
    /// # Panics
    /// Panics if `at > self.len()`.
    pub fn insert(&mut self, at: usize, text: &str) {
        assert!(at <= self.total_bytes, "insert offset out of bounds");
        if text.is_empty() {
            return;
        }
        self.nl_insert(at, text);

        let add_off = self.add.len();
        self.add.push_str(text);
        let np = Piece {
            source: Source::Add,
            offset: add_off,
            length: text.len(),
        };

        if self.pieces.is_empty() || at == self.total_bytes {
            self.pieces.push(np);
        } else {
            let (idx, off) = self.find_piece(at);
            if off == 0 {
                self.pieces.insert(idx, np);
            } else {
                let p = self.pieces[idx].clone();
                let left = Piece {
                    source: p.source,
                    offset: p.offset,
                    length: off,
                };
                let right = Piece {
                    source: p.source,
                    offset: p.offset + off,
                    length: p.length - off,
                };
                self.pieces.splice(idx..=idx, [left, np, right]);
            }
        }
        self.total_bytes += text.len();
    }

    /// Delete the bytes in `range`.
    ///
    /// # Panics
    /// Panics if `range.end > self.len()`.
    pub fn delete(&mut self, range: Range<usize>) {
        assert!(range.end <= self.total_bytes, "delete out of bounds");
        if range.start == range.end {
            return;
        }
        self.nl_delete(&range);

        let mut new_pieces: Vec<Piece> = Vec::new();
        let mut pos = 0usize;
        for piece in &self.pieces {
            let pe = pos + piece.length;
            if pe <= range.start || pos >= range.end {
                new_pieces.push(piece.clone());
            } else if pos >= range.start && pe <= range.end {
                // fully inside deleted range — drop
            } else {
                if pos < range.start {
                    new_pieces.push(Piece {
                        source: piece.source,
                        offset: piece.offset,
                        length: range.start - pos,
                    });
                }
                if pe > range.end {
                    let skip = range.end - pos;
                    new_pieces.push(Piece {
                        source: piece.source,
                        offset: piece.offset + skip,
                        length: pe - range.end,
                    });
                }
            }
            pos = pe;
        }
        self.pieces = new_pieces;
        self.total_bytes -= range.end - range.start;
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn src(&self, s: Source) -> &str {
        match s {
            Source::Original => &self.original,
            Source::Add => &self.add,
        }
    }

    /// Returns `(piece_index, offset_within_piece)` for byte offset `at`.
    /// Only called when `at < total_bytes` and pieces is non-empty.
    fn find_piece(&self, at: usize) -> (usize, usize) {
        let mut pos = 0usize;
        for (i, piece) in self.pieces.iter().enumerate() {
            if at < pos + piece.length {
                return (i, at - pos);
            }
            if at == pos + piece.length {
                return (i + 1, 0);
            }
            pos += piece.length;
        }
        (self.pieces.len(), 0)
    }

    fn nl_insert(&mut self, at: usize, text: &str) {
        for p in &mut self.newline_offsets {
            if *p >= at {
                *p += text.len();
            }
        }
        let ins = self.newline_offsets.partition_point(|&p| p < at);
        let new_nl: Vec<usize> = text
            .char_indices()
            .filter(|(_, c)| *c == '\n')
            .map(|(i, _)| at + i)
            .collect();
        self.newline_offsets.splice(ins..ins, new_nl);
    }

    fn nl_delete(&mut self, range: &Range<usize>) {
        let len = range.end - range.start;
        self.newline_offsets
            .retain(|&p| p < range.start || p >= range.end);
        for p in &mut self.newline_offsets {
            if *p >= range.end {
                *p -= len;
            }
        }
    }
}

impl From<&str> for PieceTable {
    /// Create a `PieceTable` pre-loaded with the content of `s`.
    fn from(s: &str) -> Self {
        Self::load(s)
    }
}

impl From<String> for PieceTable {
    /// Create a `PieceTable` pre-loaded with the content of `s`.
    fn from(s: String) -> Self {
        Self::load(&s)
    }
}

impl Default for PieceTable {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PieceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.slice(0..self.total_bytes))
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table() {
        let t = PieceTable::new();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
        assert_eq!(t.line_count(), 0);
    }

    #[test]
    fn from_str_round_trips() {
        let t = PieceTable::from("hello\nworld");
        assert_eq!(t.to_string(), "hello\nworld");
        assert_eq!(t.len(), 11);
        assert_eq!(t.line_count(), 2);
    }

    #[test]
    fn insert_at_end() {
        let mut t = PieceTable::from("hello");
        t.insert(5, " world");
        assert_eq!(t.to_string(), "hello world");
    }

    #[test]
    fn insert_at_start() {
        let mut t = PieceTable::from("world");
        t.insert(0, "hello ");
        assert_eq!(t.to_string(), "hello world");
    }

    #[test]
    fn insert_in_middle_splits_piece() {
        let mut t = PieceTable::from("helloworld");
        t.insert(5, " ");
        assert_eq!(t.to_string(), "hello world");
        assert_eq!(t.pieces.len(), 3);
    }

    #[test]
    fn delete_from_middle() {
        let mut t = PieceTable::from("hello world");
        t.delete(5..6);
        assert_eq!(t.to_string(), "helloworld");
    }

    #[test]
    fn delete_entire_content() {
        let mut t = PieceTable::from("hello");
        t.delete(0..5);
        assert_eq!(t.to_string(), "");
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn delete_spans_pieces() {
        let mut t = PieceTable::new();
        t.insert(0, "hello");
        t.insert(5, " world");
        // Now: [Add:"hello", Add:" world"]
        assert_eq!(t.to_string(), "hello world");
        t.delete(3..8); // delete "lo wo"
        assert_eq!(t.to_string(), "helrld");
    }

    #[test]
    fn slice_single_piece() {
        let t = PieceTable::from("hello world");
        let s = t.slice(6..11);
        assert!(matches!(s, Cow::Borrowed(_)));
        assert_eq!(s, "world");
    }

    #[test]
    fn line_count_and_line_at() {
        let t = PieceTable::from("foo\nbar\nbaz");
        assert_eq!(t.line_count(), 3);
        assert_eq!(t.line_at(0).unwrap().as_ref(), "foo");
        assert_eq!(t.line_at(1).unwrap().as_ref(), "bar");
        assert_eq!(t.line_at(2).unwrap().as_ref(), "baz");
        assert!(t.line_at(3).is_none());
    }

    #[test]
    fn trailing_newline_adds_empty_line() {
        let t = PieceTable::from("foo\n");
        assert_eq!(t.line_count(), 2);
        assert_eq!(t.line_at(1).unwrap().as_ref(), "");
    }

    #[test]
    fn line_index_updated_on_insert() {
        let mut t = PieceTable::from("hello\nworld");
        t.insert(5, "\nmiddle");
        assert_eq!(t.to_string(), "hello\nmiddle\nworld");
        assert_eq!(t.line_count(), 3);
        assert_eq!(t.line_at(1).unwrap().as_ref(), "middle");
    }

    #[test]
    fn line_index_updated_on_delete() {
        let mut t = PieceTable::from("foo\nbar\nbaz");
        t.delete(3..8); // delete "\nbar\n"  (positions 3-7 inclusive)
        assert_eq!(t.to_string(), "foobaz");
        assert_eq!(t.line_count(), 1);
    }

    #[test]
    fn multiple_ops_agree_with_string() {
        let mut t = PieceTable::new();
        let mut s = String::new();
        let ops: &[(&str, usize, Option<usize>)] = &[
            ("hello", 0, None),
            (" world", 5, None),
            ("!", 11, None),
            ("", 0, Some(6)), // delete " world"
        ];
        for &(text, at, del_end) in ops {
            if let Some(end) = del_end {
                t.delete(at..end);
                s.drain(at..end);
            } else {
                t.insert(at, text);
                s.insert_str(at, text);
            }
            assert_eq!(t.to_string(), s, "mismatch after op");
            assert_eq!(t.len(), s.len());
        }
    }
}

// ── property tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Line count matching our model: 0 for empty, else '\n'-count + 1.
    fn ref_line_count(s: &str) -> usize {
        if s.is_empty() {
            0
        } else {
            s.matches('\n').count() + 1
        }
    }

    /// Line content matching our model (split on '\n', no trailing newline removal).
    fn ref_line_at(s: &str, i: usize) -> Option<String> {
        if s.is_empty() {
            return None;
        }
        let count = s.matches('\n').count() + 1;
        if i >= count {
            return None;
        }
        s.split('\n').nth(i).map(|l| l.to_owned())
    }

    #[derive(Clone, Debug)]
    enum Op {
        Insert { pos: u8, text: String },
        Delete { start: u8, end: u8 },
    }

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (any::<u8>(), "[a-z\n]{0,12}").prop_map(|(pos, text)| Op::Insert { pos, text }),
            (any::<u8>(), any::<u8>()).prop_map(|(s, e)| Op::Delete { start: s, end: e }),
        ]
    }

    proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig {
            cases: 1000,
            ..Default::default()
        })]

        #[test]
        fn piece_table_matches_string_reference(
            ops in proptest::collection::vec(arb_op(), 1..=40)
        ) {
            let mut pt = PieceTable::new();
            let mut r = String::new();

            for op in ops {
                match op {
                    Op::Insert { pos, text } => {
                        let at = if r.is_empty() {
                            0
                        } else {
                            pos as usize % (r.len() + 1)
                        };
                        pt.insert(at, &text);
                        r.insert_str(at, &text);
                    }
                    Op::Delete { start, end } => {
                        if r.is_empty() {
                            continue;
                        }
                        let s = start as usize % r.len();
                        let span = end as usize % (r.len() - s + 1);
                        pt.delete(s..s + span);
                        r.drain(s..s + span);
                    }
                }
                prop_assert_eq!(pt.len(), r.len(), "len mismatch");
                prop_assert_eq!(pt.to_string(), r.as_str(), "content mismatch");
            }

            // Line-level verification after all ops.
            let lc = ref_line_count(&r);
            prop_assert_eq!(pt.line_count(), lc, "line_count mismatch");
            for i in 0..lc {
                prop_assert_eq!(
                    pt.line_at(i).map(|c| c.into_owned()),
                    ref_line_at(&r, i),
                    "line_at mismatch"
                );
            }

            // Slice the whole buffer.
            if !r.is_empty() {
                let full = pt.slice(0..r.len());
                prop_assert_eq!(full.as_ref(), r.as_str(), "full slice mismatch");
            }
        }
    }
}
