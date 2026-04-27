//! Entry point for the vasek-edit terminal editor.

mod app;
mod logging;
mod ui;

use std::io;
use std::path::PathBuf;

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor_core::Buffer;
use ratatui::{backend::CrosstermBackend, Terminal};

fn main() -> anyhow::Result<()> {
    let path = parse_args()?;

    logging::init().context("failed to initialise logging")?;
    tracing::info!(path = %path.display(), "opening file");

    let buffer =
        Buffer::from_file(&path).with_context(|| format!("failed to load {}", path.display()))?;

    let app = app::App::new(buffer);
    let mut terminal = setup_terminal()?;

    let run_result = run_app(&mut terminal, &app);
    let restore_result = restore_terminal(&mut terminal);

    // Propagate the run error first; only surface restore error on clean exit.
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
    app: &app::App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        match event::read()? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') => {
                    tracing::info!("quit");
                    break;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    tracing::info!("quit via ctrl-c");
                    break;
                }
                _ => {}
            },
            Event::Resize(_, _) => {} // ratatui redraws automatically on next draw call
            _ => {}
        }
    }
    Ok(())
}
