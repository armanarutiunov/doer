//! The two written rows below the list: the search/status line, and the mode bar.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use doer_core::MainMode;
use doer_core::text;

use crate::ui::theme::Theme;

pub struct StatusView<'a> {
    pub mode: MainMode,
    /// Query and caret column, while a search is being typed.
    pub search: Option<(&'a str, usize)>,
    /// A transient message. It borrows the search row, so it wins while it stands.
    pub toast: Option<(&'a str, Style)>,
    pub done: usize,
    pub total: usize,
    pub show_help: bool,
}

pub fn render_search(frame: &mut Frame, area: Rect, view: &StatusView<'_>, theme: &Theme) {
    if area.is_empty() {
        return;
    }

    // The toast only reaches here when the mode allows it (see `Screen::resolve`).
    if let Some((message, style)) = view.toast {
        frame.render_widget(
            Paragraph::new(Line::styled(message.to_string(), style)),
            area,
        );
        return;
    }

    let Some((query, caret)) = view.search else {
        return;
    };
    frame.render_widget(
        Paragraph::new(Line::styled(format!("/{query}"), theme.search)),
        area,
    );
    let x = area.x + text::to_u16(caret + 1);
    frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
}

pub fn render_modebar(frame: &mut Frame, area: Rect, view: &StatusView<'_>, theme: &Theme) {
    if area.is_empty() {
        return;
    }

    let (label, style) = match view.mode {
        MainMode::Normal => ("NORMAL", theme.mode_normal),
        MainMode::Visual => ("VISUAL", theme.mode_visual),
        MainMode::Insert => ("INSERT", theme.mode_insert),
        MainMode::Search | MainMode::SearchNav => ("SEARCH", theme.mode_search),
    };
    let mode = format!(" {label} ");
    let count = format!("{}/{} completed", view.done, view.total);
    let hint = if view.mode == MainMode::Normal && !view.show_help {
        "? for help"
    } else {
        ""
    };

    let remaining = text::to_usize(area.width)
        .saturating_sub(text::width(&mode))
        .saturating_sub(text::width(&count))
        .saturating_sub(text::width(hint));
    let left_gap = remaining / 2;

    let line = Line::from(vec![
        Span::styled(mode, style),
        Span::raw(" ".repeat(left_gap)),
        Span::styled(count, theme.meta),
        Span::raw(" ".repeat(remaining - left_gap)),
        Span::styled(hint, theme.meta),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
