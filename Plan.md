# Rust Text Editor — Feature Plan

A terminal-based text editor in Rust, built around a PieceTable buffer for efficient editing and undo/redo. Features ship in numbered phases; each phase has an explicit acceptance criterion and must be green before the next begins.

## Tech stack

- **Rendering**: `ratatui` (TUI framework)
- **Terminal I/O**: `crossterm`
- **Errors**: `thiserror` for `editor-core`, `anyhow` for the binary
- **Logging**: `tracing` + `tracing-subscriber` (writes to a file, never stdout — stdout belongs to the TUI)
- **Testing**: built-in `#[test]` + `proptest` for the PieceTable

## Workspace layout

```
text-editor/
├── Cargo.toml          # workspace root
├── CLAUDE.md
├── PLAN.md
├── crates/
│   ├── editor-core/    # buffer, cursor, edit ops, undo/redo — no rendering
│   └── editor-tui/     # ratatui frontend, event loop, layout
└── tests/              # cross-crate integration tests
```

The split exists so a future GUI frontend (egui, gpui, …) can reuse `editor-core` unchanged.

---

## Phase 1 — Foundation

**Goal**: a TUI binary that opens a file and displays it read-only.

- Cargo workspace with `editor-core` and `editor-tui`
- Logging to `~/.cache/text-editor/log` (or platform equivalent)
- Application event loop with a clean shutdown path
- CLI: `text-editor <PATH>` opens the file
- Read-only display with line numbers
- `q` quits

**Acceptance**: `cargo run -- README.md` shows the file with numbered lines; `q` exits cleanly with no terminal corruption.

## Phase 2 — PieceTable buffer

**Goal**: all text state lives in a PieceTable in `editor-core`, fully covered by tests.

- `original` (immutable, from disk) + `add` (append-only) buffers
- `Vec<Piece>` sequence with `{ source, offset, length }`
- Public ops: `insert(offset, &str)`, `delete(Range<usize>)`, `len() -> usize`, `slice(Range<usize>) -> Cow<str>`, `line_at(usize)`, `line_count()`
- Line-offset index, incrementally maintained on edits
- Property-based tests: random sequences of inserts/deletes vs. a `String` reference implementation must agree on every read

**Acceptance**: `cargo test -p editor-core` passes including ≥1000 proptest cases; `editor-core` has no `ratatui`/`crossterm` deps.

## Phase 3 — Editing and file I/O

**Goal**: a working editor that round-trips files.

- Cursor (line, column) with: arrows, home/end, page up/down, word jumps (`ctrl+left/right`)
- Insert mode editing: typed chars, backspace, delete, enter, tab
- Load: UTF-8 with BOM detection, line-ending detection (preserve `\n` vs `\r\n` on save)
- Save: atomic write (write to temp + rename)
- "Modified" flag, prompt on quit if dirty (`:q!` to force)

**Acceptance**: opening a file, making no edits, and saving produces a byte-identical file.

## Phase 4 — Display features

- **Numbered lines** gutter, toggle keybinding
- **Soft text wrap** toggle — wraps visually only, never modifies the buffer
- Vertical and horizontal scrolling that follows the cursor
- Status line: filename, dirty marker, cursor `(line:col)`, mode, file encoding

**Acceptance**: wrapping a 10k-line file is smooth; toggling wrap preserves cursor position.

## Phase 5 — Undo/redo

- Command pattern: each edit is an `EditOp` with `apply(&mut Buffer)` and `invert() -> EditOp`
- `History` with undo and redo stacks
- Coalesce consecutive single-character inserts into one unit (flush on cursor move, newline, or 500 ms idle)
- New edit after undo clears the redo stack
- Bounded history (configurable, default 1000 ops)

**Acceptance**: invariant test — `apply` then `undo` returns the buffer to the exact prior state for any sequence.

## Phase 6 — Scrollbar minimap

- Narrow right-side column rendering a condensed view of the whole buffer (1 char per N source lines, density auto-fit to viewport)
- Highlighted band shows the current viewport region
- Mouse: click jumps the viewport; drag scrolls

**Acceptance**: minimap stays in sync during edits and scrolls; click-to-jump lands on the expected line ±1.

---

## Future phases (out of initial scope)

- **Git integration** — `git2` for repo discovery, gutter markers (added / modified / deleted), inline blame
- **Folder viewer** — collapsible file tree on the left, open file on enter
- **Split views** — horizontal and vertical splits, each with its own viewport over a buffer
- **Tabs** — multiple open buffers, ring-switch keybinding, dirty indicator per tab

These will get their own phase docs once Phase 6 lands.

---

## Cross-cutting non-goals (for now)

- Syntax highlighting
- LSP / completions
- Search and replace (will be added between Phase 4 and Phase 5 if time permits)
- Plugin system
- Configuration file (hardcode keybindings until the feature set stabilises)