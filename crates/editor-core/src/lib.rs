//! Core library for the vasek-edit text editor.
//!
//! Contains all buffer management, cursor logic, and edit operations.
//! Has no dependency on any rendering framework so alternative frontends
//! can reuse it unchanged.

pub mod buffer;
pub mod error;

pub use buffer::Buffer;
pub use error::EditorError;
