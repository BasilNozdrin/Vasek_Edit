//! Terminal rendering for vasek-edit.
//!
//! Layout (top to bottom):
//!   - Editor area  (variable height, fills available space)
//!   - Status bar   (1 row)
//!   - Command line (1 row)

use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
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

// ── editor area ──────────────────────────────────────────────────────────────

fn render_editor(frame: &mut Frame, app: &App, area: Rect) {
    let lc = app.doc.line_count();
    let gutter_w = if app.show_line_numbers {
        gutter_width(lc) + 1
    } else {
        0
    };
    let height = area.height as usize;

    let lines: Vec<Line<'static>> = (app.scroll_top..app.scroll_top + height)
        .map(|li| build_editor_line(app, li, lc, gutter_w))
        .collect();

    let mut para = Paragraph::new(lines);
    if app.soft_wrap {
        para = para.wrap(Wrap { trim: false });
    } else {
        // Horizontal scroll is applied by trimming the leading bytes in each
        // line span — ratatui's Paragraph has no built-in x-scroll, so we
        // handle it in build_editor_line.
    }
    frame.render_widget(para, area);
}

fn build_editor_line(app: &App, li: usize, lc: usize, gutter_w: usize) -> Line<'static> {
    if li >= lc {
        let tilde = if app.show_line_numbers {
            format!("{:>width$} ", "~", width = gutter_w - 1)
        } else {
            String::new()
        };
        return Line::from(vec![Span::styled(
            tilde,
            Style::default().fg(Color::DarkGray),
        )]);
    }

    let raw = app
        .doc
        .line_at(li)
        .map(|c| c.into_owned())
        .unwrap_or_default();

    // Apply horizontal scroll (byte-level clip, soft-wrap ignores it).
    let visible: String = if app.soft_wrap || app.scroll_left == 0 {
        raw
    } else if app.scroll_left < raw.len() {
        // Advance to a valid char boundary at or after scroll_left.
        let mut sl = app.scroll_left;
        while sl < raw.len() && !raw.is_char_boundary(sl) {
            sl += 1;
        }
        raw[sl..].to_owned()
    } else {
        String::new()
    };

    let num_span = if app.show_line_numbers {
        Span::styled(
            format!("{:>width$} ", li + 1, width = gutter_w - 1),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::raw("")
    };

    Line::from(vec![num_span, Span::raw(visible)])
}

// ── status bar ───────────────────────────────────────────────────────────────

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let mode_str = match &app.mode {
        Mode::Normal => " NORMAL ",
        Mode::Insert => " INSERT ",
        Mode::Command(_) => " COMMAND",
    };
    let dirty = if app.doc.is_dirty() { "[+] " } else { "" };
    let filename = app
        .doc
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_owned();
    let enc = app.encoding_label();
    let wrap_ind = if app.soft_wrap { " WRAP" } else { "" };
    let pos = format!(" {}:{}", app.doc.cursor.line + 1, app.doc.cursor.col + 1);

    let left = Span::styled(
        mode_str,
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let file_span = Span::styled(
        format!("  {dirty}{filename}  "),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );
    let right_str = format!("{enc}{wrap_ind}{pos} ");
    let right = Span::styled(
        right_str.clone(),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );
    let filler_len = (area.width as usize)
        .saturating_sub(mode_str.len() + dirty.len() + filename.len() + 4 + right_str.len());
    let filler = Span::styled(
        " ".repeat(filler_len),
        Style::default().bg(Color::DarkGray).fg(Color::White),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![left, file_span, filler, right])),
        area,
    );
}

// ── command line ─────────────────────────────────────────────────────────────

fn render_cmdline(frame: &mut Frame, app: &App, area: Rect) {
    let content: String = match &app.mode {
        Mode::Command(cmd) => format!(":{cmd}"),
        _ if !app.message.is_empty() => app.message.clone(),
        _ => String::new(),
    };
    frame.render_widget(Paragraph::new(content).block(Block::default()), area);
}

// ── cursor placement ─────────────────────────────────────────────────────────

fn place_cursor(frame: &mut Frame, app: &App, editor_area: Rect, cmd_area: Rect) {
    match &app.mode {
        Mode::Command(cmd) => {
            frame.set_cursor_position(Position::new(cmd_area.x + 1 + cmd.len() as u16, cmd_area.y));
        }
        _ => {
            let row = app.doc.cursor.line.saturating_sub(app.scroll_top);
            if row >= editor_area.height as usize {
                return;
            }
            let lc = app.doc.line_count();
            let gutter_w = if app.show_line_numbers {
                gutter_width(lc) + 1
            } else {
                0
            };
            // Visual column: byte col minus the horizontal scroll offset.
            let vcol = app.doc.cursor.col.saturating_sub(app.scroll_left);
            let col = gutter_w + vcol;
            frame.set_cursor_position(Position::new(
                (editor_area.x + col as u16).min(editor_area.right().saturating_sub(1)),
                editor_area.y + row as u16,
            ));
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Width (in characters) needed for the line-number gutter, excluding the
/// trailing space (that is added separately).
fn gutter_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len()
}

/// Visible-column width of the editor text area.
pub fn text_area_width(area: Rect, show_line_numbers: bool, line_count: usize) -> usize {
    let gw = if show_line_numbers {
        gutter_width(line_count) + 1
    } else {
        0
    };
    (area.width as usize).saturating_sub(gw)
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

    #[test]
    fn text_area_width_with_gutter() {
        let area = Rect::new(0, 0, 80, 24);
        // 4-digit gutter (1000+ lines) + space = 5 cols → 75 text cols
        assert_eq!(text_area_width(area, true, 1000), 75);
        assert_eq!(text_area_width(area, false, 1000), 80);
    }
}
