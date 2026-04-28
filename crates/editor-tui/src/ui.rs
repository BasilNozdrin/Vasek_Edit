//! Terminal rendering for vasek-edit.

use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

/// Render the full editor UI onto `frame`.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let editor_area = chunks[0];
    let status_area = chunks[1];
    let cmd_area = chunks[2];

    render_editor(frame, app, editor_area);
    render_status(frame, app, status_area);
    render_cmdline(frame, app, cmd_area);
    place_cursor(frame, app, editor_area, cmd_area);
}

fn render_editor(frame: &mut Frame, app: &App, area: Rect) {
    let lc = app.doc.line_count();
    let gutter_w = gutter_width(lc);
    let height = area.height as usize;

    let lines: Vec<Line<'static>> = (app.scroll_top..app.scroll_top + height)
        .map(|li| {
            if li < lc {
                let text = app
                    .doc
                    .line_at(li)
                    .map(|c| c.into_owned())
                    .unwrap_or_default();
                let num = Span::styled(
                    format!("{:>gutter_w$} ", li + 1),
                    Style::default().fg(Color::DarkGray),
                );
                Line::from(vec![num, Span::raw(text)])
            } else {
                // Lines past end of file — show tilde like vi.
                let tilde = Span::styled(
                    format!("{:>gutter_w$} ", "~"),
                    Style::default().fg(Color::DarkGray),
                );
                Line::from(vec![tilde])
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let mode_label = match &app.mode {
        Mode::Normal => " NORMAL ",
        Mode::Insert => " INSERT ",
        Mode::Command(_) => " COMMAND",
    };
    let dirty = if app.doc.is_dirty() { " [+]" } else { "" };
    let filename = app
        .doc
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    let pos = format!(" {}:{} ", app.doc.cursor.line + 1, app.doc.cursor.col + 1);

    let left = Span::styled(
        mode_label,
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let mid = Span::styled(
        format!("  {filename}{dirty}  "),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );
    let right = Span::styled(
        pos.clone(),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );
    let filler = Span::styled(
        " ".repeat(area.width.saturating_sub(
            (mode_label.len() + filename.len() + dirty.len() + 4 + pos.len()) as u16,
        ) as usize),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![left, mid, filler, right])),
        area,
    );
}

fn render_cmdline(frame: &mut Frame, app: &App, area: Rect) {
    let content = match &app.mode {
        Mode::Command(cmd) => format!(":{cmd}"),
        _ if !app.message.is_empty() => app.message.clone(),
        _ => String::new(),
    };
    frame.render_widget(Paragraph::new(content).block(Block::default()), area);
}

fn place_cursor(frame: &mut Frame, app: &App, editor_area: Rect, cmd_area: Rect) {
    match &app.mode {
        Mode::Command(cmd) => {
            // Cursor in the command line, after the `:` prompt.
            frame.set_cursor_position(Position::new(cmd_area.x + 1 + cmd.len() as u16, cmd_area.y));
        }
        _ => {
            // Cursor in the editor area.
            let row = app.doc.cursor.line.saturating_sub(app.scroll_top);
            if row >= editor_area.height as usize {
                return; // cursor scrolled off-screen
            }
            let lc = app.doc.line_count();
            let gutter_w = gutter_width(lc);
            // col is a byte offset; treat as visual column (ASCII assumption for Phase 3).
            let col = gutter_w + 1 + app.doc.cursor.col;
            frame.set_cursor_position(Position::new(
                (editor_area.x + col as u16).min(editor_area.right().saturating_sub(1)),
                editor_area.y + row as u16,
            ));
        }
    }
}

fn gutter_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_values() {
        assert_eq!(gutter_width(0), 1);
        assert_eq!(gutter_width(9), 1);
        assert_eq!(gutter_width(10), 2);
        assert_eq!(gutter_width(100), 3);
    }
}
