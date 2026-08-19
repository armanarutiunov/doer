#![allow(clippy::unwrap_used, clippy::panic, clippy::string_slice)]

//! Golden frames. The snapshots are the visual-parity contract with the Elixir build,
//! so a diff here means the screen changed for the user.

use std::collections::HashMap;

use doer::ui::draw;
use doer::ui::theme::Theme;
use doer_core::app::{AppState, MainState, Pane, SidebarCursor};
use doer_core::display::ViewId;
use doer_core::layout::Geometry;
use doer_core::text::TextInput;
use doer_core::{Project, ProjectId, Projects, Todo, TodoId, Workspace};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

const NOW: i64 = 1_700_000_000;
const DAY: i64 = 86_400;

fn todo(id: &str, text: &str, age_days: i64) -> Todo {
    Todo {
        id: TodoId::from(id),
        text: text.into(),
        done: false,
        created_at: NOW - age_days * DAY,
        completed_at: None,
    }
}

fn done(id: &str, text: &str, age_days: i64, completed_days: i64) -> Todo {
    Todo {
        done: true,
        completed_at: Some(NOW - completed_days * DAY),
        ..todo(id, text, age_days)
    }
}

fn workspace() -> Workspace {
    let work = ProjectId::from("1111111111111111");
    let deep = ProjectId::from("2222222222222222");
    let projects = Projects::new(vec![
        Project {
            id: work.clone(),
            name: "Work".into(),
            index: 0,
            parent_id: None,
        },
        Project {
            id: deep.clone(),
            name: "Deep".into(),
            index: 0,
            parent_id: Some(work.clone()),
        },
    ]);

    let mut project_todos = HashMap::new();
    project_todos.insert(
        work,
        vec![todo(
            "aaaaaaaaaaaaaaa1",
            "a deliberately long todo that has to wrap across more than one line to prove the scroll model counts them",
            12,
        )],
    );
    project_todos.insert(
        deep,
        vec![todo("aaaaaaaaaaaaaaa2", "nested project todo", 3)],
    );

    Workspace::new(
        vec![
            todo("bbbbbbbbbbbbbbb1", "buy milk", 1),
            todo("bbbbbbbbbbbbbbb2", "call the plumber", 5),
            done("ccccccccccccccc1", "ship the port", 30, 2),
        ],
        projects,
        project_todos,
    )
}

struct Fixture {
    app: AppState,
}

impl Fixture {
    fn new(width: u16, height: u16) -> Self {
        let mut app = AppState::new(workspace(), Geometry::new(width, height, true));
        app.cursor = Some(TodoId::from("bbbbbbbbbbbbbbb1"));
        Self { app }
    }

    fn render(&self) -> Terminal<TestBackend> {
        let (width, height) = (self.app.geo.term_width, self.app.geo.term_height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw(frame, &self.app, NOW, &Theme::purple()))
            .unwrap();
        terminal
    }
}

/// Rows as plain text with the right edge marked, so trailing padding is visible in a
/// diff without snapshotting a style grid.
fn text_of(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            format!("{row}|")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn find(buffer: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        if let Some(byte) = row.find(needle) {
            let column = row[..byte].chars().count();
            return (u16::try_from(column).unwrap(), y);
        }
    }
    panic!("{needle:?} not on screen");
}

#[test]
fn frame_80x24() {
    let terminal = Fixture::new(80, 24).render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_100x30() {
    let terminal = Fixture::new(100, 30).render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_56x20_is_the_narrowest_screen_that_keeps_the_sidebar() {
    let terminal = Fixture::new(56, 20).render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_40x10_drops_the_sidebar() {
    let terminal = Fixture::new(40, 10).render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_20x5() {
    let terminal = Fixture::new(20, 5).render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_with_the_help_overlay() {
    let mut fixture = Fixture::new(100, 30);
    fixture.app.help = true;
    let terminal = fixture.render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_in_a_project_view_with_the_sidebar_focused() {
    let project = ProjectId::from("1111111111111111");
    let mut fixture = Fixture::new(80, 24);
    fixture.app.view = ViewId::Project(project.clone());
    fixture.app.cursor = Some(TodoId::from("aaaaaaaaaaaaaaa1"));
    fixture.app.pane = Pane::Sidebar;
    fixture.app.sidebar_cursor = SidebarCursor::Project(project);
    let terminal = fixture.render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn frame_while_searching() {
    let mut fixture = Fixture::new(80, 24);
    fixture.app.main = MainState::Search(TextInput::new("the"));
    let terminal = fixture.render();
    insta::assert_snapshot!(text_of(terminal.backend().buffer()));
}

#[test]
fn a_completed_todo_is_struck_through() {
    let terminal = Fixture::new(120, 40).render();
    let buffer = terminal.backend().buffer();
    let (x, y) = find(buffer, "ship the port");
    assert!(
        buffer[(x, y)]
            .style()
            .add_modifier
            .contains(Modifier::CROSSED_OUT)
    );
}

#[test]
fn the_visual_selection_bar_is_pink_on_the_first_line_only() {
    let mut fixture = Fixture::new(120, 40);
    fixture.app.main = MainState::Visual {
        anchor: TodoId::from("bbbbbbbbbbbbbbb1"),
    };
    let terminal = fixture.render();
    let buffer = terminal.backend().buffer();

    let (x, y) = find(buffer, "▎");
    assert_eq!(
        buffer[(x, y)].style().fg,
        Some(Theme::purple().selection_bar.fg.unwrap())
    );

    let (todo_x, todo_y) = find(buffer, "buy milk");
    assert_eq!(y, todo_y);
    assert!(todo_x > x);
}

#[test]
fn the_cursor_row_carries_the_highlight_background() {
    let terminal = Fixture::new(120, 40).render();
    let buffer = terminal.backend().buffer();
    let (x, y) = find(buffer, "buy milk");
    assert_eq!(buffer[(x, y)].style().bg, Theme::purple().cursor_row.bg);
}

#[test]
fn every_line_of_a_wrapped_todo_carries_the_highlight() {
    let mut fixture = Fixture::new(120, 40);
    fixture.app.cursor = Some(TodoId::from("aaaaaaaaaaaaaaa1"));
    let terminal = fixture.render();
    let buffer = terminal.backend().buffer();

    let (x, y) = find(buffer, "a deliberately");
    let highlight = Theme::purple().cursor_row.bg;
    assert_eq!(buffer[(x, y)].style().bg, highlight);
    assert_eq!(
        buffer[(x, y + 1)].style().bg,
        highlight,
        "continuation line"
    );
}
