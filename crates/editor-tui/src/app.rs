//! Application state for vasek-edit.

use editor_core::Document;

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
    /// Index of the first visible line (scroll offset).
    pub scroll_top: usize,
    /// One-line message displayed in the command area (errors, hints).
    pub message: String,
    /// Set to `true` when the event loop should exit.
    pub should_quit: bool,
}

impl App {
    /// Create a new `App` wrapping `doc`, starting in Normal mode.
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            mode: Mode::Normal,
            scroll_top: 0,
            message: String::new(),
            should_quit: false,
        }
    }

    /// Ensure `scroll_top` keeps the cursor inside the visible editor area.
    pub fn scroll_to_cursor(&mut self, editor_height: usize) {
        if editor_height == 0 {
            return;
        }
        let line = self.doc.cursor.line;
        if line < self.scroll_top {
            self.scroll_top = line;
        } else if line >= self.scroll_top + editor_height {
            self.scroll_top = line - editor_height + 1;
        }
    }
}
