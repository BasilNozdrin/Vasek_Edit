//! Core library for the vasek-edit text editor.
//!
//! Contains all buffer management, cursor logic, and edit operations.
//! Has no dependency on any rendering framework so alternative frontends
//! can reuse it unchanged.

pub mod buffer;
pub mod cursor;
pub mod document;
pub mod error;
pub mod piece_table;

pub use buffer::Buffer;
pub use cursor::Cursor;
pub use document::{Document, LineEnding};
pub use error::EditorError;
pub use piece_table::PieceTable;
