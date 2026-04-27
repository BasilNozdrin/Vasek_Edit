//! Top-level application state.

use editor_core::Buffer;

/// Application state passed to the rendering layer each frame.
pub struct App {
    buffer: Buffer,
}

impl App {
    /// Create a new `App` wrapping `buffer`.
    pub fn new(buffer: Buffer) -> Self {
        Self { buffer }
    }

    /// Returns a reference to the loaded buffer.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }
}
