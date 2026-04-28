//! Entry point for the vasek-edit terminal editor.

mod app;
mod logging;
mod ui;

use std::io;
use std::path::PathBuf;

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor_core::Document;
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Mode};

fn main() -> anyhow::Result<()> {
    let path = parse_args()?;

    logging::init().context("failed to initialise logging")?;
    tracing::info!(path = %path.display(), "opening file");

    let doc =
        Document::open(&path).with_context(|| format!("failed to load {}", path.display()))?;
    let mut app = App::new(doc);

    let mut terminal = setup_terminal()?;
    let run_result = run_app(&mut terminal, &mut app);
    let restore_result = restore_terminal(&mut terminal);

    run_result?;
    restore_result?;
    Ok(())
}

fn parse_args() -> anyhow::Result<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    match args.as_slice() {
        [_, path] => Ok(PathBuf::from(path)),
        [bin, ..] => anyhow::bail!("usage: {} <PATH>", bin),
        [] => anyhow::bail!("usage: vasek-edit <PATH>"),
    }
}

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        let editor_height = terminal.size()?.height.saturating_sub(2) as usize;
        app.scroll_to_cursor(editor_height);
        terminal.draw(|frame| ui::render(frame, app))?;

        if let Event::Key(key) = event::read()? {
            handle_key(app, key);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    app.message.clear();
    match app.mode.clone() {
        Mode::Normal => handle_normal(app, key),
        Mode::Insert => handle_insert(app, key),
        Mode::Command(cmd) => handle_command(app, key, cmd),
    }
}

// ── Normal mode ──────────────────────────────────────────────────────────────

fn handle_normal(app: &mut App, key: KeyEvent) {
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            if app.doc.is_dirty() {
                app.message = "Unsaved changes — use :w to save, :q! to discard".into();
            } else {
                app.should_quit = true;
            }
        }
        // Enter command mode
        KeyCode::Char(':') => {
            app.mode = Mode::Command(String::new());
        }
        // Enter insert mode
        KeyCode::Char('i') => {
            app.mode = Mode::Insert;
        }
        // Ctrl-S quick save
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            save(app);
        }
        // Cursor movement — guarded arms must precede unguarded ones.
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => app.doc.word_left(),
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => app.doc.word_right(),
        KeyCode::Up | KeyCode::Char('k') => app.doc.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.doc.move_down(),
        KeyCode::Left | KeyCode::Char('h') => app.doc.move_left(),
        KeyCode::Right | KeyCode::Char('l') => app.doc.move_right(),
        KeyCode::Home => app.doc.move_home(),
        KeyCode::End => app.doc.move_end(),
        KeyCode::PageUp => {
            let ph = page_height(app);
            app.doc.page_up(ph);
        }
        KeyCode::PageDown => {
            let ph = page_height(app);
            app.doc.page_down(ph);
        }
        _ => {}
    }
}

// ── Insert mode ──────────────────────────────────────────────────────────────

fn handle_insert(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.doc.insert_at_cursor("\n");
        }
        KeyCode::Tab => {
            app.doc.insert_at_cursor("\t");
        }
        KeyCode::Backspace => {
            app.doc.backspace();
        }
        KeyCode::Delete => {
            app.doc.delete_forward();
        }
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => app.doc.word_left(),
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => app.doc.word_right(),
        KeyCode::Up => app.doc.move_up(),
        KeyCode::Down => app.doc.move_down(),
        KeyCode::Left => app.doc.move_left(),
        KeyCode::Right => app.doc.move_right(),
        KeyCode::Home => app.doc.move_home(),
        KeyCode::End => app.doc.move_end(),
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            app.doc.insert_at_cursor(c.encode_utf8(&mut buf));
        }
        _ => {}
    }
}

// ── Command mode ─────────────────────────────────────────────────────────────

fn handle_command(app: &mut App, key: KeyEvent, mut cmd: String) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            cmd.pop();
            app.mode = Mode::Command(cmd);
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            execute_command(app, &cmd);
        }
        KeyCode::Char(c) => {
            cmd.push(c);
            app.mode = Mode::Command(cmd);
        }
        _ => {}
    }
}

fn execute_command(app: &mut App, cmd: &str) {
    match cmd.trim() {
        "w" | "write" => save(app),
        "q" | "quit" => {
            if app.doc.is_dirty() {
                app.message = "Unsaved changes — use :w first, or :q! to discard".into();
            } else {
                app.should_quit = true;
            }
        }
        "q!" | "quit!" => {
            app.should_quit = true;
        }
        "wq" | "x" => {
            save(app);
            app.should_quit = true;
        }
        other => {
            app.message = format!("Unknown command: {other}");
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn save(app: &mut App) {
    match app.doc.save() {
        Ok(()) => {
            let name = app
                .doc
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            app.message = format!("Saved {name}");
            tracing::info!("saved {}", app.doc.path().display());
        }
        Err(e) => {
            app.message = format!("Save failed: {e}");
            tracing::error!("save failed: {e}");
        }
    }
}

fn page_height(app: &App) -> usize {
    // Best-effort: we don't have terminal size here, so use scroll_top context.
    // The real height is passed into scroll_to_cursor each frame.
    let _ = app;
    20
}
