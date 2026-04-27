//! File-based tracing subscriber setup.
//!
//! Logs are written to the platform cache directory so they never appear on
//! stdout (which belongs to the TUI).
//!
//! Log file locations:
//! - Windows:  `%LOCALAPPDATA%\vasek-edit\vasek-edit.log`
//! - macOS:    `~/Library/Caches/vasek-edit/vasek-edit.log`
//! - Linux:    `~/.cache/vasek-edit/vasek-edit.log`

use std::fs::{self, OpenOptions};
use std::sync::Mutex;

use anyhow::Context;

/// Initialise the global tracing subscriber, directing output to a log file.
///
/// Must be called once before any `tracing::*` macros are used.
pub fn init() -> anyhow::Result<()> {
    let log_dir = dirs::cache_dir()
        .context("cannot determine platform cache directory")?
        .join("vasek-edit");

    fs::create_dir_all(&log_dir)
        .with_context(|| format!("cannot create log directory {}", log_dir.display()))?;

    let log_path = log_dir.join("vasek-edit.log");
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("cannot open log file {}", log_path.display()))?;

    tracing_subscriber::fmt()
        .with_writer(Mutex::new(log_file))
        .with_ansi(false)
        .init();

    Ok(())
}
