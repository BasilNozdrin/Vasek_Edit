//! Application state for vasek-edit.

use editor_core::{Document, LineEnding};

/// Editor operating mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// Accumulates a colon-command (e.g. `"w"`, `"q!"`, `"wq"`).
    Command(String),
}

/// Top-level application state.
pub struct App {
    pub doc: Document,
    pub mode: Mode,
    /// Index of the first visible line (vertical scroll offset).
    pub scroll_top: usize,
    /// Horizontal scroll offset in bytes (how many leading bytes are hidden).
    pub scroll_left: usize,
    /// One-line message displayed in the command area (errors, hints).
    pub message: String,
    /// Set to `true` when the event loop should exit.
    pub should_quit: bool,
    /// Whether the line-number gutter is visible.
    pub show_line_numbers: bool,
    /// Whether soft word-wrap is active.
    pub soft_wrap: bool,
}

impl App {
    /// Create a new `App` wrapping `doc`, starting in Normal mode.
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            mode: Mode::Normal,
            scroll_top: 0,
            scroll_left: 0,
            message: String::new(),
            should_quit: false,
            show_line_numbers: true,
            soft_wrap: false,
        }
    }

    /// Ensure `scroll_top` and `scroll_left` keep the cursor inside the
    /// visible editor area.
    pub fn scroll_to_cursor(&mut self, editor_height: usize, editor_width: usize) {
        if editor_height == 0 {
            return;
        }
        // Vertical.
        let line = self.doc.cursor.line;
        if line < self.scroll_top {
            self.scroll_top = line;
        } else if line >= self.scroll_top + editor_height {
            self.scroll_top = line - editor_height + 1;
        }
        // Horizontal (only when soft-wrap is off).
        if !self.soft_wrap && editor_width > 0 {
            let col = self.doc.cursor.col;
            if col < self.scroll_left {
                self.scroll_left = col;
            } else if col >= self.scroll_left + editor_width {
                self.scroll_left = col - editor_width + 1;
            }
        } else {
            self.scroll_left = 0;
        }
    }

    /// Human-readable encoding/line-ending label for the status bar.
    pub fn encoding_label(&self) -> &'static str {
        match self.doc.line_ending() {
            LineEnding::Lf => "UTF-8 LF",
            LineEnding::CrLf => "UTF-8 CRLF",
        }
    }
}
