use std::collections::HashMap;
use std::ops::Range;

use crate::display::{DisplayList, ViewId};
use crate::id::TodoId;
use crate::text;
use crate::workspace::{Bucket, Workspace};

pub const SIDEBAR_WIDTH: u16 = 35;
pub const BORDER_WIDTH: u16 = 1;
pub const CONTENT_PERCENT: u16 = 60;
pub const CONTENT_MIN_WIDTH: u16 = 20;
pub const PAD_Y_TOP: u16 = 1;
pub const BOTTOM_RESERVED: u16 = 5;
pub const SCROLL_MARGIN: usize = 5;
pub const PREFIX_WIDTH: u16 = 4;

const AGE_COLUMN: usize = 7;
/// The text is the point of a row, so a date column is shrunk and then dropped rather
/// than squeezing the text below this. Twelve columns is roughly two short words, which
/// is where wrapping stops being readable.
const MIN_TEXT_WIDTH: usize = 12;
/// Enough for "999d", which is as wide as a real age label gets.
const NARROW_COLUMN: usize = 4;
const COMPLETED_COLUMN: usize = 9;
pub const EMPTY_HINT: &str = "press 'a' to add a new todo";

/// Terminal geometry, and the derived widths every layout decision reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub term_width: u16,
    pub term_height: u16,
    pub sidebar_open: bool,
}

impl Geometry {
    #[must_use]
    pub fn new(term_width: u16, term_height: u16, sidebar_open: bool) -> Self {
        Self {
            term_width,
            term_height,
            sidebar_open,
        }
    }

    /// The sidebar hides itself when the main column could not keep its minimum
    /// width. This is a per-frame display decision and never changes the flag.
    #[must_use]
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_open && self.term_width >= SIDEBAR_WIDTH + BORDER_WIDTH + CONTENT_MIN_WIDTH
    }

    #[must_use]
    pub fn available_width(&self) -> u16 {
        if self.sidebar_visible() {
            self.term_width.saturating_sub(SIDEBAR_WIDTH + BORDER_WIDTH)
        } else {
            self.term_width
        }
    }

    /// 60% of the available width, matching `trunc(available * 0.6)` exactly for
    /// every width the terminal can report, without a float.
    #[must_use]
    pub fn content_width(&self) -> u16 {
        let available = self.available_width();
        let scaled = u32::from(available) * u32::from(CONTENT_PERCENT) / 100;
        text::to_u16(scaled as usize)
            .max(CONTENT_MIN_WIDTH)
            .min(available.max(1))
    }

    /// Columns of padding left of the content column. The remainder column goes
    /// on the right, as it did in the Elixir build.
    #[must_use]
    pub fn left_pad(&self) -> u16 {
        self.available_width().saturating_sub(self.content_width()) / 2
    }

    #[must_use]
    pub fn viewport_height(&self) -> usize {
        text::to_usize(self.term_height.saturating_sub(PAD_Y_TOP + BOTTOM_RESERVED)).max(1)
    }
}

/// How much room the date columns get, and what the text keeps from what is left.
///
/// A row has to fit the content column. The Elixir version floored the text area at ten
/// columns instead, so at anything narrower than about 110 columns with the sidebar open
/// it built rows wider than the column and let the terminal clip the dates off the end.
///
/// The columns are generously padded at full width -- seven and nine, for labels that are
/// realistically three or four characters -- so a narrow terminal shrinks them to their
/// natural width before giving up and dropping them. Wide terminals keep the original
/// padding, so the alignment there is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateColumns {
    pub age: Option<usize>,
    pub completed: Option<usize>,
    pub text_width: usize,
}

impl DateColumns {
    #[must_use]
    pub fn fit(content_width: u16, has_completed: bool) -> Self {
        let budget = text::to_usize(content_width).saturating_sub(text::to_usize(PREFIX_WIDTH));

        // Widest arrangement first; the first one that leaves the text a workable
        // column wins.
        let candidates: [(Option<usize>, Option<usize>); 4] = if has_completed {
            [
                (Some(AGE_COLUMN), Some(COMPLETED_COLUMN)),
                (Some(NARROW_COLUMN), Some(NARROW_COLUMN)),
                (Some(NARROW_COLUMN), None),
                (None, None),
            ]
        } else {
            [
                (Some(AGE_COLUMN), None),
                (Some(NARROW_COLUMN), None),
                (None, None),
                (None, None),
            ]
        };

        for (age, completed) in candidates {
            let right = Self::right_width(age, completed);
            let text_width = budget.saturating_sub(right);
            if right == 0 {
                return Self {
                    age: None,
                    completed: None,
                    text_width: budget.max(1),
                };
            }
            if text_width >= MIN_TEXT_WIDTH {
                return Self {
                    age,
                    completed,
                    text_width,
                };
            }
        }
        Self {
            age: None,
            completed: None,
            text_width: budget.max(1),
        }
    }

    #[must_use]
    pub fn right_width(age: Option<usize>, completed: Option<usize>) -> usize {
        age.map_or(0, |w| 2 + w) + completed.map_or(0, |w| 2 + w)
    }

    #[must_use]
    pub fn width(self) -> usize {
        Self::right_width(self.age, self.completed)
    }

    /// The header label, spaced to sit over the columns actually drawn.
    #[must_use]
    pub fn header_label(self) -> String {
        match (self.age, self.completed) {
            (Some(age), Some(completed)) => format!(
                "{}  {}",
                text::pad_start("Created", age),
                text::pad_start("Completed", completed)
            ),
            (Some(age), None) => text::pad_start("Created", age),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Blank,
    SectionHeader { title: String, right: String },
    EmptyHint(&'static str),
    Todo(TodoRow),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoRow {
    pub id: TodoId,
    /// Index into the display order, so the renderer can test cursor and selection
    /// without a second lookup.
    pub entry_index: usize,
    pub line: String,
    pub line_index: usize,
    pub line_count: usize,
    pub done: bool,
    pub age: String,
    pub completed_age: Option<String>,
    /// Column widths the layout committed to, so the renderer pads to the same
    /// numbers the wrapping was computed against.
    pub columns: DateColumns,
    /// Column of the text caret on this line, when the todo is being edited.
    pub caret_col: Option<u16>,
}

impl TodoRow {
    #[must_use]
    pub fn is_first_line(&self) -> bool {
        self.line_index == 0
    }
}

/// What the layout needs to know about transient UI state.
#[derive(Clone, Debug, Default)]
pub struct LayoutHints<'a> {
    /// The todo being edited, with the text as typed so far and the caret column.
    pub editing: Option<(TodoId, &'a str, usize)>,
}

/// The literal list of rendered rows, and where each todo sits in it.
///
/// This is the only coordinate system for scrolling. Wrapped continuation lines,
/// section headers and blank spacers are all rows here, so nothing downstream has
/// to re-derive a visual position and get it wrong.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub rows: Vec<Row>,
    pub spans: HashMap<TodoId, Range<usize>>,
    pub order: Vec<TodoId>,
    pub content_width: u16,
}

impl Layout {
    #[must_use]
    pub fn build(
        ws: &Workspace,
        dl: &DisplayList<'_>,
        view: &ViewId,
        geo: &Geometry,
        hints: &LayoutHints<'_>,
        now: i64,
    ) -> Self {
        let content_width = geo.content_width();
        let mut builder = Builder {
            rows: Vec::new(),
            spans: HashMap::new(),
            order: Vec::new(),
            content_width,
        };

        if dl.sectioned {
            builder.active_sections(ws, dl, hints, now);
        } else {
            builder.push(Row::SectionHeader {
                title: view_title(ws, view),
                right: DateColumns::fit(content_width, false).header_label(),
            });
            builder.push(Row::Blank);
            if dl.active.is_empty() {
                builder.push(Row::EmptyHint(EMPTY_HINT));
            } else {
                for (offset, entry) in dl.active.iter().enumerate() {
                    builder.todo(entry.todo, offset, false, hints, now);
                }
            }
        }

        if !dl.completed.is_empty() {
            builder.push(Row::Blank);
            builder.push(Row::Blank);
            builder.push(Row::SectionHeader {
                title: "Completed".into(),
                right: DateColumns::fit(content_width, true).header_label(),
            });
            builder.push(Row::Blank);
            let offset_base = dl.active.len();
            for (offset, entry) in dl.completed.iter().enumerate() {
                builder.todo(entry.todo, offset_base + offset, true, hints, now);
            }
        }

        Self {
            rows: builder.rows,
            spans: builder.spans,
            order: builder.order,
            content_width,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Row range of a todo, including every wrapped continuation line.
    #[must_use]
    pub fn span(&self, id: &TodoId) -> Option<Range<usize>> {
        self.spans.get(id).cloned()
    }

    #[must_use]
    pub fn index_of(&self, id: &TodoId) -> Option<usize> {
        self.order.iter().position(|other| other == id)
    }

    #[must_use]
    pub fn id_at(&self, index: usize) -> Option<&TodoId> {
        self.order.get(index)
    }
}

struct Builder {
    rows: Vec<Row>,
    spans: HashMap<TodoId, Range<usize>>,
    order: Vec<TodoId>,
    content_width: u16,
}

impl Builder {
    fn push(&mut self, row: Row) {
        self.rows.push(row);
    }

    fn active_sections(
        &mut self,
        ws: &Workspace,
        dl: &DisplayList<'_>,
        hints: &LayoutHints<'_>,
        now: i64,
    ) {
        let mut offset = 0;
        let mut group_index = 0;
        while offset < dl.active.len() {
            let Some(first) = dl.active.get(offset) else {
                break;
            };
            let bucket = first.bucket.clone();
            let group_len = dl.active[offset..]
                .iter()
                .take_while(|entry| entry.bucket == bucket)
                .count();

            if group_index > 0 {
                self.push(Row::Blank);
                self.push(Row::Blank);
            }
            self.push(Row::SectionHeader {
                title: section_title(ws, &bucket),
                right: DateColumns::fit(self.content_width, false).header_label(),
            });
            self.push(Row::Blank);

            for local in 0..group_len {
                let Some(entry) = dl.active.get(offset + local) else {
                    break;
                };
                self.todo(entry.todo, offset + local, false, hints, now);
            }

            offset += group_len;
            group_index += 1;
        }
    }

    fn todo(
        &mut self,
        todo: &crate::todo::Todo,
        entry_index: usize,
        done: bool,
        hints: &LayoutHints<'_>,
        now: i64,
    ) {
        let age = format!("{}d", todo.age_days(now));
        let completed_age = if done {
            todo.completed_days(now).map(|days| format!("{days}d"))
        } else {
            None
        };

        // The completed section carries a second date column, so its rows wrap narrower
        // than active ones. Wrapping here is what keeps the scroll model aware of that.
        let columns = DateColumns::fit(self.content_width, completed_age.is_some());
        let age = if columns.age.is_some() {
            age
        } else {
            String::new()
        };
        let completed_age = completed_age.filter(|_| columns.completed.is_some());
        let text_width = columns.text_width;

        let editing = hints.editing.as_ref().filter(|(id, _, _)| *id == todo.id);
        let source = editing.map_or(todo.text.as_str(), |(_, text, _)| *text);
        let lines = text::wrap(source, text_width);
        let caret = editing.map(|(_, _, col)| *col);

        let start = self.rows.len();
        let line_count = lines.len();
        let mut consumed = 0;
        for (line_index, line) in lines.into_iter().enumerate() {
            let line_width = text::width(&line);
            let caret_col = caret.and_then(|col| {
                let end = consumed + line_width;
                let last_line = line_index + 1 == line_count;
                // A caret sitting exactly at a wrap point belongs to the line it
                // was typed on, so it does not jump ahead of the character.
                if (col >= consumed && col < end) || (last_line && col >= end) {
                    Some(text::to_u16(col.saturating_sub(consumed)))
                } else {
                    None
                }
            });
            consumed += line_width;

            self.push(Row::Todo(TodoRow {
                id: todo.id.clone(),
                entry_index,
                line,
                line_index,
                line_count,
                done,
                age: age.clone(),
                completed_age: completed_age.clone(),
                columns,
                caret_col,
            }));
        }

        self.spans.insert(todo.id.clone(), start..self.rows.len());
        self.order.push(todo.id.clone());
    }
}

fn view_title(ws: &Workspace, view: &ViewId) -> String {
    match view {
        ViewId::All => "Todos".into(),
        ViewId::Project(id) => ws.projects().get(id).map_or_else(
            || "Todos".into(),
            |project| format!("# {} todos", project.name),
        ),
    }
}

fn section_title(ws: &Workspace, bucket: &Bucket) -> String {
    let Some(id) = bucket.project() else {
        return "Todos".into();
    };
    let Some(project) = ws.projects().get(id) else {
        return "Todos".into();
    };
    match &project.parent_id {
        None => format!("# {}", project.name),
        Some(parent) => ws.projects().get(parent).map_or_else(
            || format!("# {}", project.name),
            |p| format!("# {} / {}", p.name, project.name),
        ),
    }
}

/// Clamps an offset so the viewport never scrolls past the last row.
#[must_use]
pub fn clamp_scroll(offset: usize, rows: usize, viewport: usize) -> usize {
    offset.min(rows.saturating_sub(viewport.max(1)))
}

/// Scrolls the minimum amount that brings `span` into view, keeping a margin of
/// context above and below where the list is long enough to allow it.
///
/// Takes the whole span rather than a single row so a wrapped todo scrolls until
/// all of its lines are visible; an item taller than the viewport pins to its top
/// instead of oscillating.
#[must_use]
pub fn adjust_scroll(offset: usize, span: &Range<usize>, rows: usize, viewport: usize) -> usize {
    let viewport = viewport.max(1);
    let max_offset = rows.saturating_sub(viewport);
    let margin = SCROLL_MARGIN.min(viewport / 2);
    let mut offset = offset.min(max_offset);

    if span.start < offset.saturating_add(margin) {
        offset = span.start.saturating_sub(margin);
    }

    let fits = span.len() <= viewport.saturating_sub(margin * 2).max(1);
    let needed_end = if fits {
        span.end
    } else {
        span.start.saturating_add(1)
    };
    if needed_end.saturating_add(margin) > offset.saturating_add(viewport) {
        offset = needed_end.saturating_add(margin).saturating_sub(viewport);
    }

    offset.min(max_offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Projects;
    use crate::todo::Todo;
    use std::collections::HashMap;

    const T0: i64 = 1_700_000_000;
    const WIDE: Geometry = Geometry {
        term_width: 200,
        term_height: 40,
        sidebar_open: false,
    };

    fn empty_ws() -> Workspace {
        Workspace::new(Vec::new(), Projects::default(), HashMap::new())
    }

    fn layout_of(ws: &Workspace, geo: Geometry) -> Layout {
        let dl = DisplayList::build(ws, &ViewId::All, None);
        Layout::build(ws, &dl, &ViewId::All, &geo, &LayoutHints::default(), T0)
    }

    fn todo_rows(layout: &Layout) -> Vec<&TodoRow> {
        layout
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Todo(todo) => Some(todo),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn content_width_is_sixty_percent_without_floating_point() {
        for width in 1..=400u16 {
            let geo = Geometry::new(width, 40, false);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let expected = ((f64::from(width) * 0.6) as u16)
                .max(CONTENT_MIN_WIDTH)
                .min(width.max(1));
            assert_eq!(geo.content_width(), expected, "width {width}");
        }
    }

    #[test]
    fn the_remainder_column_of_an_odd_split_goes_on_the_right() {
        let geo = Geometry::new(81, 40, false);
        assert_eq!(geo.content_width(), 48);
        assert_eq!(geo.left_pad(), 16);
    }

    #[test]
    fn the_sidebar_hides_itself_when_the_content_column_would_not_fit() {
        assert!(Geometry::new(56, 40, true).sidebar_visible());
        assert!(!Geometry::new(55, 40, true).sidebar_visible());
        assert!(!Geometry::new(200, 40, false).sidebar_visible());
    }

    #[test]
    fn an_empty_list_still_renders_a_header_and_a_hint() {
        let ws = empty_ws();
        let layout = layout_of(&ws, WIDE);
        assert_eq!(
            layout.rows,
            [
                Row::SectionHeader {
                    title: "Todos".into(),
                    right: text::pad_start("Created", AGE_COLUMN)
                },
                Row::Blank,
                Row::EmptyHint(EMPTY_HINT),
            ]
        );
    }

    #[test]
    fn the_completed_block_is_preceded_by_two_blank_rows_and_its_own_header() {
        let mut ws = empty_ws();
        ws.push_todo(&Bucket::All, Todo::new("active", T0));
        let mut done = Todo::new("done", T0);
        done.toggle(T0);
        ws.push_todo(&Bucket::All, done);

        let layout = layout_of(&ws, WIDE);
        let tail = &layout.rows[layout.rows.len() - 5..];
        assert_eq!(tail[0], Row::Blank);
        assert_eq!(tail[1], Row::Blank);
        assert_eq!(
            tail[2],
            Row::SectionHeader {
                title: "Completed".into(),
                right: DateColumns::fit(WIDE.content_width(), true).header_label()
            }
        );
        assert_eq!(tail[3], Row::Blank);
        assert!(matches!(&tail[4], Row::Todo(todo) if todo.done));
    }

    #[test]
    fn sections_separate_projects_with_two_blank_rows() {
        let mut ws = empty_ws();
        let a = ws.add_project("Alpha".into(), None);
        let b = ws.add_project("Beta".into(), None);
        ws.push_todo(&Bucket::All, Todo::new("loose", T0));
        ws.push_todo(&Bucket::Project(a), Todo::new("alpha work", T0));
        ws.push_todo(&Bucket::Project(b), Todo::new("beta work", T0));

        let layout = layout_of(&ws, WIDE);
        let headers: Vec<&str> = layout
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::SectionHeader { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(headers, ["Todos", "# Alpha", "# Beta"]);

        let second = layout
            .rows
            .iter()
            .position(|row| matches!(row, Row::SectionHeader { title, .. } if title == "# Alpha"))
            .expect("header");
        assert_eq!(layout.rows[second - 2], Row::Blank);
        assert_eq!(layout.rows[second - 1], Row::Blank);
    }

    #[test]
    fn a_child_project_header_names_its_parent() {
        let mut ws = empty_ws();
        let parent = ws.add_project("Work".into(), None);
        let child = ws.add_project("Api".into(), Some(parent));
        ws.push_todo(&Bucket::Project(child), Todo::new("ship it", T0));

        let layout = layout_of(&ws, WIDE);
        assert!(
            layout.rows.iter().any(
                |row| matches!(row, Row::SectionHeader { title, .. } if title == "# Work / Api")
            )
        );
    }

    #[test]
    fn a_wrapped_todo_occupies_a_span_of_several_rows() {
        let mut ws = empty_ws();
        let todo = Todo::new(
            "a rather long todo that will certainly need to wrap somewhere",
            T0,
        );
        let id = todo.id.clone();
        ws.push_todo(&Bucket::All, todo);

        let geo = Geometry::new(60, 40, false);
        let layout = layout_of(&ws, geo);
        let span = layout.span(&id).expect("span");
        assert!(span.len() > 1, "expected the todo to wrap, got {span:?}");
        assert_eq!(todo_rows(&layout).len(), span.len());
    }

    #[test]
    fn the_same_text_wraps_narrower_once_completed() {
        let text = "some text that sits right around the wrapping boundary for this width";
        let geo = Geometry::new(60, 40, false);

        let mut active_ws = empty_ws();
        let active = Todo::new(text, T0);
        let active_id = active.id.clone();
        active_ws.push_todo(&Bucket::All, active);

        let mut done_ws = empty_ws();
        let mut done = Todo::new(text, T0);
        done.toggle(T0);
        let done_id = done.id.clone();
        done_ws.push_todo(&Bucket::All, done);

        let active_lines = layout_of(&active_ws, geo)
            .span(&active_id)
            .expect("span")
            .len();
        let done_lines = layout_of(&done_ws, geo).span(&done_id).expect("span").len();
        assert!(
            done_lines > active_lines,
            "completed rows carry a second date column so they must wrap sooner: {done_lines} vs {active_lines}"
        );
    }

    #[test]
    fn the_caret_lands_on_the_line_that_holds_it() {
        let mut ws = empty_ws();
        let todo = Todo::new("", T0);
        let id = todo.id.clone();
        ws.push_todo(&Bucket::All, todo);

        let typed = "aaaa bbbb cccc dddd eeee ffff";
        let geo = Geometry::new(60, 40, false);
        let dl = DisplayList::build(&ws, &ViewId::All, None);
        let hints = LayoutHints {
            editing: Some((id.clone(), typed, text::width(typed))),
        };
        let layout = Layout::build(&ws, &dl, &ViewId::All, &geo, &hints, T0);

        let carets: Vec<(usize, u16)> = todo_rows(&layout)
            .iter()
            .filter_map(|row| row.caret_col.map(|col| (row.line_index, col)))
            .collect();
        assert_eq!(carets.len(), 1, "exactly one line carries the caret");
        let (line_index, _) = carets[0];
        assert_eq!(
            line_index,
            todo_rows(&layout).len() - 1,
            "caret is on the last line"
        );
    }

    #[test]
    fn order_lists_active_todos_before_completed_ones() {
        let mut ws = empty_ws();
        ws.push_todo(&Bucket::All, Todo::new("active", T0));
        let mut done = Todo::new("done", T0);
        done.toggle(T0);
        ws.push_todo(&Bucket::All, done);

        let layout = layout_of(&ws, WIDE);
        let texts: Vec<Option<&Todo>> = layout.order.iter().map(|id| ws.get(id)).collect();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].map(|t| t.text.as_str()), Some("active"));
        assert_eq!(texts[1].map(|t| t.text.as_str()), Some("done"));
    }

    #[test]
    fn scrolling_keeps_a_margin_above_the_cursor() {
        assert_eq!(adjust_scroll(20, &(10..11), 100, 20), 5);
    }

    #[test]
    fn scrolling_keeps_a_margin_below_the_cursor() {
        assert_eq!(adjust_scroll(0, &(18..19), 100, 20), 4);
    }

    #[test]
    fn a_cursor_already_in_view_does_not_move_the_offset() {
        assert_eq!(adjust_scroll(10, &(20..21), 100, 20), 10);
    }

    #[test]
    fn scrolling_reveals_every_line_of_a_wrapped_todo() {
        let offset = adjust_scroll(0, &(15..18), 100, 20);
        assert!(offset + 20 >= 18 + SCROLL_MARGIN.min(10));
    }

    #[test]
    fn a_todo_taller_than_the_viewport_pins_to_its_top() {
        let offset = adjust_scroll(0, &(10..40), 100, 20);
        assert!(
            offset <= 10,
            "expected the top of the item to stay visible, got {offset}"
        );
    }

    #[test]
    fn the_offset_never_scrolls_past_the_last_row() {
        assert_eq!(adjust_scroll(90, &(99..100), 100, 20), 80);
        assert_eq!(clamp_scroll(90, 100, 20), 80);
        assert_eq!(clamp_scroll(5, 3, 20), 0);
    }

    #[test]
    fn shrinking_the_terminal_pulls_a_stale_offset_back_into_range() {
        let rows = 40;
        assert_eq!(adjust_scroll(30, &(0..1), rows, 10), 0);
        assert_eq!(clamp_scroll(35, rows, 10), 30);
    }

    #[test]
    fn a_viewport_of_one_row_still_terminates_and_stays_in_range() {
        let offset = adjust_scroll(0, &(50..51), 100, 1);
        assert!(offset < 100);
    }
}

#[cfg(test)]
mod narrow_tests {
    use super::*;
    use crate::project::Projects;
    use crate::todo::Todo;
    use std::collections::HashMap;

    const T0: i64 = 1_700_000_000;

    fn rows_for(text: &str, done: bool, term_width: u16) -> Vec<TodoRow> {
        let mut ws = Workspace::new(Vec::new(), Projects::default(), HashMap::new());
        let mut todo = Todo::new(text, T0);
        if done {
            todo.toggle(T0);
        }
        ws.push_todo(&Bucket::All, todo);

        let geo = Geometry::new(term_width, 40, false);
        let dl = DisplayList::build(&ws, &ViewId::All, None);
        Layout::build(&ws, &dl, &ViewId::All, &geo, &LayoutHints::default(), T0)
            .rows
            .into_iter()
            .filter_map(|row| match row {
                Row::Todo(todo) => Some(todo),
                _ => None,
            })
            .collect()
    }

    /// The whole row -- prefix, text, gap and date columns -- has to fit the content
    /// column, or the terminal clips whatever hangs over the edge.
    fn assert_fits(term_width: u16, done: bool) {
        let geo = Geometry::new(term_width, 40, false);
        let content = text::to_usize(geo.content_width());
        for row in rows_for("a todo with several words in it", done, term_width) {
            let used = text::to_usize(PREFIX_WIDTH) + text::width(&row.line) + row.columns.width();
            assert!(
                used <= content,
                "width {term_width}: row uses {used} of {content} columns ({:?})",
                row.line
            );
        }
    }

    /// Below this the renderer draws a "terminal too small" notice instead of the list,
    /// so the row arithmetic is not asked to fit a column narrower than the checkbox.
    const SMALLEST_USABLE: u16 = 20;

    #[test]
    fn a_row_always_fits_the_content_column() {
        for width in SMALLEST_USABLE..=200u16 {
            assert_fits(width, false);
            assert_fits(width, true);
        }
    }

    #[test]
    fn the_date_columns_shrink_before_either_is_dropped() {
        let wide = rows_for("short", true, 200);
        let wide = wide.first().expect("a row").columns;
        assert_eq!(wide.age, Some(AGE_COLUMN));
        assert_eq!(wide.completed, Some(COMPLETED_COLUMN));

        let narrow = rows_for("short", true, 52);
        let narrow = narrow.first().expect("a row").columns;
        assert_eq!(
            narrow.age,
            Some(NARROW_COLUMN),
            "both columns survive, narrower"
        );
        assert_eq!(narrow.completed, Some(NARROW_COLUMN));
    }

    #[test]
    fn the_completed_column_goes_before_the_age_column() {
        let rows = rows_for("short", true, 40);
        let columns = rows.first().expect("a row").columns;
        assert_eq!(
            columns.completed, None,
            "the second date is the first to go"
        );
        assert!(columns.age.is_some(), "the age column survives longer");
    }

    /// Narrower than the renderer's own floor, but the layout must still produce a
    /// row that fits rather than one the terminal has to clip.
    #[test]
    fn a_terminal_too_narrow_for_any_date_keeps_only_the_text() {
        let rows = rows_for("short", true, 35);
        let first = rows.first().expect("a row");
        assert_eq!(first.columns.age, None);
        assert_eq!(first.columns.completed, None);
        assert!(!first.line.is_empty());
    }

    #[test]
    fn a_header_only_advertises_the_columns_that_are_drawn() {
        assert_eq!(DateColumns::fit(20, true).header_label(), "");
        assert_eq!(DateColumns::fit(26, true).header_label().trim(), "Created");
        assert_eq!(DateColumns::fit(40, false).header_label().trim(), "Created");
        assert_eq!(
            DateColumns::fit(200, true).header_label().trim(),
            "Created  Completed"
        );
    }

    #[test]
    fn a_wide_terminal_keeps_both_date_columns() {
        let rows = rows_for("short", true, 200);
        let first = rows.first().expect("a row");
        assert!(!first.age.is_empty());
        assert!(first.completed_age.is_some());
    }
}
