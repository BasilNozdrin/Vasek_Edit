//! Terminal rendering for vasek-edit.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

/// Render the read-only editor view onto `frame`.
pub fn render(frame: &mut Frame, app: &App) {
    let buf = app.buffer();
    let gutter_w = gutter_width(buf.line_count());

    let lines: Vec<Line<'static>> = buf
        .lines()
        .iter()
        .enumerate()
        .map(|(i, text)| numbered_line(i + 1, text, gutter_w))
        .collect();

    frame.render_widget(Paragraph::new(lines), frame.area());
}

/// Build a single line with a dim line-number gutter on the left.
fn numbered_line(number: usize, text: &str, gutter_w: usize) -> Line<'static> {
    let num = Span::styled(
        format!("{number:>gutter_w$} "),
        Style::default().fg(Color::DarkGray),
    );
    let content = Span::raw(text.to_owned());
    Line::from(vec![num, content])
}

/// Number of decimal digits needed to display `line_count`.
fn gutter_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_values() {
        assert_eq!(gutter_width(0), 1);
        assert_eq!(gutter_width(1), 1);
        assert_eq!(gutter_width(9), 1);
        assert_eq!(gutter_width(10), 2);
        assert_eq!(gutter_width(99), 2);
        assert_eq!(gutter_width(100), 3);
        assert_eq!(gutter_width(1000), 4);
    }
}
