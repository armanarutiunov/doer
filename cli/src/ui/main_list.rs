//! The todo list. Draws exactly the rows `Layout` produced — the slice
//! `rows[scroll..scroll + height]` and nothing else, so what is scrolled and what is
//! painted can never disagree.

use std::ops::RangeInclusive;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use doer_core::TodoId;
use doer_core::layout::{Layout, PREFIX_WIDTH, Row, TodoRow};
use doer_core::text;

use crate::ui::theme::Theme;

/// Everything the list needs from the app, resolved once by `ui::draw`.
pub struct ListView<'a> {
    pub layout: &'a Layout,
    pub scroll: usize,
    pub cursor: Option<&'a TodoId>,
    /// Entry indices covered by a visual-mode selection.
    pub selection: Option<RangeInclusive<usize>>,
    /// The todo whose text is being edited, if any.
    pub editing: Option<&'a TodoId>,
    /// False when the sidebar holds the keyboard, which dims the cursor row.
    pub main_focused: bool,
}

pub fn render(frame: &mut Frame, area: Rect, view: &ListView<'_>, theme: &Theme) {
    if area.is_empty() {
        return;
    }

    let height = text::to_usize(area.height);
    let end = view
        .scroll
        .saturating_add(height)
        .min(view.layout.rows.len());
    let visible = view.layout.rows.get(view.scroll..end).unwrap_or_default();
    let content_width = text::to_usize(view.layout.content_width);

    let mut caret = None;
    let lines: Vec<Line<'static>> = visible
        .iter()
        .enumerate()
        .map(|(offset, row)| {
            if let Row::Todo(todo) = row
                && let Some(col) = todo.caret_col
            {
                caret = Some((area.x + PREFIX_WIDTH + col, area.y + text::to_u16(offset)));
            }
            paint(row, content_width, view, theme)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);

    if let Some((x, y)) = caret {
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), y));
    }
}

fn paint(row: &Row, content_width: usize, view: &ListView<'_>, theme: &Theme) -> Line<'static> {
    match row {
        Row::Blank => Line::default(),
        Row::SectionHeader { title, right } => section_header(title, right, content_width, theme),
        Row::EmptyHint(hint) => Line::from(vec![
            Span::raw(" ".repeat(text::to_usize(PREFIX_WIDTH))),
            Span::styled((*hint).to_string(), theme.dim),
        ]),
        Row::Todo(todo) => todo_line(todo, content_width, view, theme),
    }
}

fn section_header(title: &str, right: &str, content_width: usize, theme: &Theme) -> Line<'static> {
    let title_width = content_width
        .saturating_sub(text::to_usize(PREFIX_WIDTH))
        .saturating_sub(text::width(right))
        .saturating_sub(2);
    Line::from(vec![
        Span::raw(" ".repeat(text::to_usize(PREFIX_WIDTH))),
        Span::styled(text::pad_end(title, title_width), theme.dim),
        Span::styled(format!("  {right}"), theme.dim),
    ])
}

fn todo_line(
    todo: &TodoRow,
    content_width: usize,
    view: &ListView<'_>,
    theme: &Theme,
) -> Line<'static> {
    let is_cursor = view.cursor == Some(&todo.id);
    let is_selected = view
        .selection
        .as_ref()
        .is_some_and(|range| range.contains(&todo.entry_index));
    let is_editing = view.editing == Some(&todo.id);

    let right = right_column(todo);
    let right_width = text::width(&right);
    let text_width = content_width
        .saturating_sub(text::to_usize(PREFIX_WIDTH))
        .saturating_sub(right_width)
        .max(10);

    let text_style = if is_editing {
        theme.editing
    } else if todo.done {
        theme.done_text
    } else if is_cursor {
        theme.cursor_text
    } else {
        theme.text
    };

    let mut spans = Vec::with_capacity(5);
    if todo.is_first_line() {
        spans.push(Span::styled(
            if is_selected { "▎ " } else { "  " },
            if is_selected {
                theme.selection_bar
            } else {
                theme.text
            },
        ));
        spans.push(Span::raw(if todo.done { "◉ " } else { "◯ " }));
    } else {
        spans.push(Span::raw(" ".repeat(text::to_usize(PREFIX_WIDTH))));
    }

    spans.push(Span::styled(
        text::pad_end(&todo.line, text_width),
        text_style,
    ));
    spans.push(Span::styled(
        if todo.is_first_line() {
            right
        } else {
            " ".repeat(right_width)
        },
        theme.meta,
    ));

    let line = Line::from(spans);
    match cursor_row_style(is_cursor, is_editing, view, theme) {
        Some(style) => line.style(style),
        None => line,
    }
}

/// The highlight covers every line of a wrapped todo, and is skipped while editing —
/// in the degraded theme it inverts, and a second inversion inside it would cancel out.
fn cursor_row_style(
    is_cursor: bool,
    is_editing: bool,
    view: &ListView<'_>,
    theme: &Theme,
) -> Option<Style> {
    if !is_cursor || is_editing {
        return None;
    }
    Some(if view.main_focused {
        theme.cursor_row
    } else {
        theme.cursor_row_unfocused
    })
}

fn right_column(todo: &TodoRow) -> String {
    let mut right = match todo.columns.age {
        Some(width) => format!("  {}", text::pad_start(&todo.age, width)),
        None => String::new(),
    };
    if let (Some(completed), Some(width)) = (&todo.completed_age, todo.columns.completed) {
        right.push_str("  ");
        right.push_str(&text::pad_start(completed, width));
    }
    right
}
