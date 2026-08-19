//! The `?` overlay. The bindings themselves live in `doer_core::help`, next to the
//! keymap that defines them; this only lays them out.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use doer_core::help::{Section, column_height, columns, key_width};
use doer_core::text;

use crate::ui::theme::Theme;

const PAD_X: usize = 3;
const COL_WIDTH: usize = 30;
const GAP: usize = 2;

/// The whole screen is cleared before the panel is drawn. Leaving the list showing
/// beside it left orphaned age values and wrapped tails hanging in space, which reads
/// as a rendering fault rather than as deliberate transparency; and clearing the screen
/// rather than just the list keeps the panel its full width on a terminal too narrow
/// for the panel to fit beside the sidebar.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    if area.is_empty() {
        return;
    }
    frame.render_widget(Clear, area);

    let [left, right] = columns();
    let rows = column_height(left).max(column_height(right)) + 2;
    let box_width = PAD_X + COL_WIDTH + GAP + COL_WIDTH + PAD_X;

    let height = text::to_u16(rows).min(area.height);
    let width = text::to_u16(box_width).min(area.width);
    if width == 0 || height == 0 {
        return;
    }

    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);

    frame.render_widget(Paragraph::new(lines(rows, theme)), popup);
}

fn lines(rows: usize, theme: &Theme) -> Vec<Line<'static>> {
    let [left, right] = columns().map(|sections| flatten(sections, rows));
    let pad = " ".repeat(PAD_X);
    let gap = " ".repeat(GAP);

    left.into_iter()
        .zip(right)
        .map(|(left, right)| {
            Line::from(vec![
                Span::styled(pad.clone(), theme.help_surface),
                cell(left, theme),
                Span::styled(gap.clone(), theme.help_surface),
                cell(right, theme),
                Span::styled(pad.clone(), theme.help_surface),
            ])
        })
        .collect()
}

/// One blank row of breathing space above each section title, and one blank row at the
/// top and bottom of the panel.
fn flatten(sections: &[Section], rows: usize) -> Vec<Cell> {
    let gutter = key_width() + 2;
    let mut out = vec![Cell::Blank];
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            out.push(Cell::Blank);
        }
        out.push(Cell::Title(section.title));
        out.push(Cell::Blank);
        for (keys, description) in section.bindings {
            out.push(Cell::Binding(format!(
                "{}{description}",
                text::pad_end(keys, gutter)
            )));
        }
    }
    out.resize_with(rows, || Cell::Blank);
    out
}

enum Cell {
    Blank,
    Title(&'static str),
    Binding(String),
}

fn cell(cell: Cell, theme: &Theme) -> Span<'static> {
    let (body, style) = match cell {
        Cell::Blank => (String::new(), theme.help_text),
        Cell::Title(title) => (title.to_string(), theme.help_heading),
        Cell::Binding(body) => (body, theme.help_text),
    };
    Span::styled(text::pad_end(&body, COL_WIDTH), style)
}
