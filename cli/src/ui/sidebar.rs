//! The project pane: "All Todos", a `Projects` heading, then the flat project list.
//!
//! Its rows are built the same way the main list's are — a literal `Vec` with the
//! cursor's position known by index — so the scroll the Elixir build carried as dead
//! state can actually work here.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use doer_core::app::ProjectEdit;
use doer_core::layout::{SIDEBAR_WIDTH, clamp_scroll};
use doer_core::{ProjectId, Projects, text};

use crate::ui::theme::Theme;

/// Rows above the first project: "All Todos", blank, "Projects", blank.
pub const PREAMBLE_ROWS: usize = 4;
const INDENT: usize = 2;
const PROJECT_PREFIX: &str = "# ";
const HINT: &str = "press 'a' to create";

pub struct SidebarView<'a> {
    pub projects: &'a Projects,
    /// `None` is the "All Todos" row. An identity rather than a row number, so a
    /// cascading delete cannot leave it pointing at whatever slid into the slot.
    pub cursor: Option<&'a ProjectId>,
    /// The project currently being viewed, or `None` for the All view.
    pub current: Option<&'a ProjectId>,
    pub focused: bool,
    pub editing: Option<(&'a ProjectEdit, &'a str, usize)>,
    pub confirm_delete: Option<&'a ProjectId>,
}

enum SidebarRow {
    Blank,
    Heading(&'static str),
    Hint(&'static str),
    AllTodos,
    Project {
        id: ProjectId,
        depth: usize,
    },
    /// A project being renamed, or a new one being named.
    Editing {
        depth: usize,
    },
}

pub fn render(frame: &mut Frame, area: Rect, view: &SidebarView<'_>, theme: &Theme) {
    if area.is_empty() {
        return;
    }

    let rows = build_rows(view);
    let height = text::to_usize(area.height);
    let cursor_row = cursor_row(view, &rows);
    let scroll = scroll_offset(cursor_row, rows.len(), height);

    let end = scroll.saturating_add(height).min(rows.len());
    let visible = rows.get(scroll..end).unwrap_or_default();
    let width = text::to_usize(area.width.min(SIDEBAR_WIDTH));

    let mut caret = None;
    let lines: Vec<Line<'static>> = visible
        .iter()
        .enumerate()
        .map(|(offset, row)| {
            let (line, caret_col) = paint(row, scroll + offset == cursor_row, width, view, theme);
            if let Some(col) = caret_col {
                caret = Some((area.x + col, area.y + text::to_u16(offset)));
            }
            line
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);

    if let Some((x, y)) = caret {
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), y));
    }
}

/// A long project list scrolls to keep the cursor on screen. Derived per frame rather
/// than stored: the sidebar has no notion of a scroll position the user chose, so
/// there is nothing to remember and nothing to clamp after a resize.
fn scroll_offset(cursor_row: usize, rows: usize, height: usize) -> usize {
    let height = height.max(1);
    let offset = if cursor_row >= height {
        cursor_row + 1 - height
    } else {
        0
    };
    clamp_scroll(offset, rows, height)
}

fn build_rows(view: &SidebarView<'_>) -> Vec<SidebarRow> {
    let mut rows = vec![
        SidebarRow::AllTodos,
        SidebarRow::Blank,
        SidebarRow::Heading("Projects"),
        SidebarRow::Blank,
    ];

    let flat = view.projects.flat_ordered();
    let editing_existing = match &view.editing {
        Some((ProjectEdit::Rename(id), _, _)) => Some(id.clone()),
        _ => None,
    };

    for project in &flat {
        let depth = usize::from(project.parent_id.is_some());
        if editing_existing.as_ref() == Some(&project.id) {
            rows.push(SidebarRow::Editing { depth });
        } else {
            rows.push(SidebarRow::Project {
                id: project.id.clone(),
                depth,
            });
        }
    }

    match &view.editing {
        Some((ProjectEdit::NewTopLevel, _, _)) => rows.push(SidebarRow::Editing { depth: 0 }),
        Some((ProjectEdit::NewChild(parent), _, _)) => {
            // Land after the parent's last existing child, which is where the new
            // project will sort once it is named.
            let last = flat
                .iter()
                .rposition(|p| p.id == *parent || p.parent_id.as_ref() == Some(parent));
            let at = PREAMBLE_ROWS + last.map_or(0, |i| i + 1);
            rows.insert(at.min(rows.len()), SidebarRow::Editing { depth: 1 });
        }
        _ => {}
    }

    if flat.is_empty() && view.editing.is_none() && view.focused {
        rows.push(SidebarRow::Hint(HINT));
    }
    rows
}

/// The row the cursor sits on. Falls back to "All Todos" when the project it names
/// has gone, which is what the reducer does with the cursor itself.
fn cursor_row(view: &SidebarView<'_>, rows: &[SidebarRow]) -> usize {
    let on_edit_row = match &view.editing {
        Some((ProjectEdit::Rename(id), _, _)) => view.cursor == Some(id),
        Some((ProjectEdit::NewTopLevel | ProjectEdit::NewChild(_), _, _)) => true,
        None => false,
    };
    if on_edit_row {
        return rows
            .iter()
            .position(|row| matches!(row, SidebarRow::Editing { .. }))
            .unwrap_or(0);
    }

    let Some(wanted) = view.cursor else {
        return 0;
    };
    rows.iter()
        .position(|row| matches!(row, SidebarRow::Project { id, .. } if id == wanted))
        .unwrap_or(0)
}

fn label_indent(depth: usize) -> usize {
    INDENT * (depth + 1)
}

/// Where a project name starts on its row: the indent plus the "# " every row draws.
fn name_column(depth: usize) -> usize {
    label_indent(depth) + PROJECT_PREFIX.len()
}

/// Returns the row, and the caret's screen column when this is the row being typed on.
fn paint(
    row: &SidebarRow,
    is_cursor: bool,
    width: usize,
    view: &SidebarView<'_>,
    theme: &Theme,
) -> (Line<'static>, Option<u16>) {
    let mut caret = None;
    let (text_body, style) = match row {
        SidebarRow::Blank => return (Line::default(), None),
        SidebarRow::Heading(label) => (format!("{}{label}", " ".repeat(INDENT)), theme.dim),
        SidebarRow::Hint(label) => (
            format!("{}{label}", " ".repeat(INDENT * 2)),
            theme.sidebar_hint,
        ),
        SidebarRow::AllTodos => (
            format!("{}All Todos", " ".repeat(INDENT)),
            if view.current.is_none() {
                theme.sidebar_current
            } else {
                theme.sidebar_item
            },
        ),
        SidebarRow::Editing { depth } => {
            let (typed, col) = view.editing.as_ref().map_or(("", 0), |(_, t, c)| (*t, *c));
            let start = name_column(*depth);
            let (visible, caret_col) = editing_window(typed, col, width.saturating_sub(start));
            caret = Some(text::to_u16(start + caret_col));
            (
                format!(
                    "{}{PROJECT_PREFIX}{visible}",
                    " ".repeat(label_indent(*depth))
                ),
                theme.editing,
            )
        }
        SidebarRow::Project { id, depth } => {
            let Some(project) = view.projects.get(id) else {
                return (Line::default(), None);
            };
            let indent = " ".repeat(label_indent(*depth));
            if view.confirm_delete == Some(id) {
                (
                    format!("{indent}{PROJECT_PREFIX}Delete? y/n"),
                    theme.confirm_delete,
                )
            } else if view.current == Some(id) {
                (
                    format!("{indent}{PROJECT_PREFIX}{}", project.name),
                    theme.sidebar_current,
                )
            } else {
                (
                    format!("{indent}{PROJECT_PREFIX}{}", project.name),
                    theme.sidebar_item,
                )
            }
        }
    };

    let body = text::pad_end(&text::truncate(&text_body, width), width);
    let line = Line::from(Span::styled(body, style));
    let line = if is_cursor {
        line.style(if view.focused {
            theme.cursor_row
        } else {
            theme.cursor_row_unfocused
        })
    } else {
        line
    };
    (line, caret)
}

/// The slice of a name being typed that keeps the caret on screen, and the caret's
/// column within that slice. A name longer than the pane scrolls under the caret
/// instead of showing its head while the tail is typed blind.
fn editing_window(name: &str, caret: usize, avail: usize) -> (String, usize) {
    // One column is reserved for the caret itself, so it stays visible when it sits
    // one past the last character.
    let room = avail.saturating_sub(1);
    if text::width(name) <= room {
        return (name.to_string(), caret);
    }
    let (visible, dropped) = skip_columns(name, caret.saturating_sub(room));
    (visible, caret.saturating_sub(dropped))
}

/// Drops whole characters until at least `columns` display columns are gone, and
/// reports how many actually went. A wide character straddling the boundary is dropped
/// whole rather than half-drawn, which is why the caller cannot assume the count.
fn skip_columns(name: &str, columns: usize) -> (String, usize) {
    let mut buf = [0u8; 4];
    let mut dropped = 0;
    let visible = name
        .chars()
        .skip_while(|ch| {
            if dropped >= columns {
                return false;
            }
            dropped += text::width(ch.encode_utf8(&mut buf));
            true
        })
        .collect();
    (visible, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_fits_is_shown_whole() {
        assert_eq!(editing_window("work", 4, 20), ("work".into(), 4));
    }

    #[test]
    fn a_long_name_scrolls_so_the_caret_stays_visible() {
        // Ten columns of room, one reserved for the caret: the last nine characters.
        let (visible, caret) = editing_window("abcdefghijklmn", 14, 10);
        assert_eq!(visible, "fghijklmn");
        assert_eq!(caret, 9);
    }

    #[test]
    fn a_caret_left_of_the_window_scrolls_back() {
        let (visible, caret) = editing_window("abcdefghijklmn", 3, 10);
        assert_eq!(visible, "abcdefghijklmn");
        assert_eq!(caret, 3);
    }

    #[test]
    fn scrolling_counts_display_columns_not_characters() {
        // Each CJK character is two columns wide, so nine columns of room fits four.
        // Room for nine columns; dropping three means dropping two whole characters,
        // so the caret moves back by the four columns that actually went.
        let (visible, caret) = editing_window("买菜买菜买菜", 12, 10);
        assert_eq!(visible, "买菜买菜");
        assert_eq!(caret, 8);
        assert_eq!(text::width(&visible), 8);
    }

    #[test]
    fn a_wide_character_on_the_boundary_is_dropped_rather_than_halved() {
        assert_eq!(skip_columns("买菜x", 1), ("菜x".into(), 2));
        assert_eq!(skip_columns("买菜x", 2), ("菜x".into(), 2));
        assert_eq!(skip_columns("买菜x", 3), ("x".into(), 4));
    }
}
