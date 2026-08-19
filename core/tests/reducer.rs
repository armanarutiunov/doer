#![allow(clippy::expect_used, clippy::panic)]

//! The reducer driven as a pure function: actions in, state and effects out.
//!
//! Nothing here builds a terminal, opens a file or looks at a clock.

use doer_core::action::{Action, Dir, EditKey, Motion, SidebarAction};
use doer_core::app::{AppState, Effect, Pane, SidebarCursor, SidebarState, ToastLevel};
use doer_core::display::ViewId;
use doer_core::layout::Geometry;
use doer_core::mode::{MainMode, SidebarMode};
use doer_core::store::Target;
use doer_core::todo::Todo;
use doer_core::workspace::{Bucket, Workspace};
use doer_core::{TodoId, app};

use Action as A;

const NOW: i64 = 1_700_000_000;

fn geometry() -> Geometry {
    Geometry::new(100, 30, true)
}

fn app(texts: &[&str]) -> AppState {
    let mut ws = Workspace::default();
    for text in texts {
        ws.push_todo(&Bucket::All, Todo::new(*text, NOW));
    }
    AppState::new(ws, geometry())
}

/// A workspace with one project so the All view is sectioned.
fn app_with_project(all: &[&str], project: &[&str]) -> (AppState, doer_core::ProjectId) {
    let mut ws = Workspace::default();
    let id = ws.add_project("Work".into(), None);
    for text in all {
        ws.push_todo(&Bucket::All, Todo::new(*text, NOW));
    }
    for text in project {
        ws.push_todo(&Bucket::Project(id.clone()), Todo::new(*text, NOW));
    }
    (AppState::new(ws, geometry()), id)
}

fn send(state: &mut AppState, action: &A) -> Vec<Effect> {
    app::reduce(state, action, NOW)
}

fn send_all(state: &mut AppState, actions: impl IntoIterator<Item = A>) {
    for action in actions {
        send(state, &action);
    }
}

fn type_text(state: &mut AppState, text: &str) {
    for ch in text.chars() {
        send(state, &A::Edit(EditKey::Char(ch)));
    }
}

fn type_sidebar(state: &mut AppState, text: &str) {
    for ch in text.chars() {
        send(state, &A::Sidebar(SidebarAction::Edit(EditKey::Char(ch))));
    }
}

fn visible(state: &AppState) -> Vec<String> {
    state
        .display()
        .order()
        .iter()
        .map(|t| t.text.clone())
        .collect()
}

fn cursor_text(state: &AppState) -> Option<String> {
    state
        .cursor
        .as_ref()
        .and_then(|id| state.ws.get(id))
        .map(|t| t.text.clone())
}

fn toast_text(effects: &[Effect]) -> Option<String> {
    effects.iter().find_map(|e| match e {
        Effect::Toast(toast) => Some(toast.text.clone()),
        _ => None,
    })
}

fn saves(effects: &[Effect]) -> Vec<Target> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Save(target) => Some(target.clone()),
            _ => None,
        })
        .collect()
}

// --- cursor ---

#[test]
fn the_cursor_starts_on_the_first_todo_and_clamps_at_both_ends() {
    let mut state = app(&["a", "b"]);
    assert_eq!(cursor_text(&state).as_deref(), Some("a"));

    send(&mut state, &A::Cursor(Motion::Up));
    assert_eq!(cursor_text(&state).as_deref(), Some("a"));

    send_all(
        &mut state,
        std::iter::repeat_with(|| A::Cursor(Motion::Down)).take(5),
    );
    assert_eq!(cursor_text(&state).as_deref(), Some("b"));
}

#[test]
fn a_half_page_jump_uses_the_viewport_not_the_terminal_height() {
    let texts: Vec<String> = (0..40).map(|i| format!("todo {i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut state = app(&refs);

    send(&mut state, &A::Cursor(Motion::HalfDown));

    // Viewport is 30 - 1 - 5 = 24 rows, so the jump is 12, not 15.
    assert_eq!(cursor_text(&state).as_deref(), Some("todo 12"));
}

#[test]
fn toggling_leaves_the_cursor_where_it_was_on_screen() {
    let mut state = app(&["a", "b", "c"]);
    send(&mut state, &A::ToggleTodo);

    assert_eq!(visible(&state), ["b", "c", "a"]);
    assert_eq!(
        cursor_text(&state).as_deref(),
        Some("b"),
        "a run of `space` should walk down the list"
    );
}

#[test]
fn deleting_leaves_the_cursor_where_it_was_on_screen() {
    let mut state = app(&["a", "b", "c"]);
    send(&mut state, &A::Cursor(Motion::Down));
    send(&mut state, &A::DeleteTodo);

    assert_eq!(visible(&state), ["a", "c"]);
    assert_eq!(cursor_text(&state).as_deref(), Some("c"));
}

#[test]
fn deleting_the_last_todo_drops_the_cursor_onto_the_new_last_row() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::Cursor(Motion::End));
    send(&mut state, &A::DeleteTodo);

    assert_eq!(cursor_text(&state).as_deref(), Some("a"));
}

#[test]
fn reordering_keeps_the_cursor_on_the_todo_that_moved() {
    let mut state = app(&["a", "b", "c"]);
    send(&mut state, &A::Move(Dir::Down));

    assert_eq!(visible(&state), ["b", "a", "c"]);
    assert_eq!(cursor_text(&state).as_deref(), Some("a"));
}

// --- editing ---

#[test]
fn adding_a_todo_inserts_it_below_the_cursor_and_commits_the_typed_text() {
    let mut state = app(&["first", "second"]);
    send(&mut state, &A::AddTodo);
    assert_eq!(state.main.mode(), MainMode::Insert);

    type_text(&mut state, "new");
    send(&mut state, &A::ConfirmEdit);

    assert_eq!(visible(&state), ["first", "new", "second"]);
    assert_eq!(state.main.mode(), MainMode::Normal);
}

#[test]
fn abandoning_a_new_todo_removes_it_and_costs_no_undo_step() {
    let mut state = app(&["only"]);
    let depth = state.undo.depth();

    send(&mut state, &A::AddTodo);
    type_text(&mut state, "half typed");
    send(&mut state, &A::CancelEdit);

    assert_eq!(visible(&state), ["only"]);
    assert_eq!(state.undo.depth(), depth);
}

#[test]
fn confirming_a_blank_new_todo_discards_it_just_as_escape_would() {
    let mut state = app(&["only"]);
    send(&mut state, &A::AddTodo);
    send(&mut state, &A::ConfirmEdit);

    assert_eq!(visible(&state), ["only"]);
}

#[test]
fn a_whole_insert_session_collapses_into_one_undo_step() {
    let mut state = app(&["only"]);
    send(&mut state, &A::AddTodo);
    type_text(&mut state, "typed slowly");
    send(&mut state, &A::ConfirmEdit);

    send(&mut state, &A::Undo);
    assert_eq!(visible(&state), ["only"]);
}

#[test]
fn emptying_an_existing_todo_reverts_it_rather_than_deleting_it() {
    let mut state = app(&["keep me"]);
    send(&mut state, &A::EditTodo);
    for _ in 0..20 {
        send(&mut state, &A::Edit(EditKey::Backspace));
    }
    send(&mut state, &A::ConfirmEdit);

    assert_eq!(visible(&state), ["keep me"]);
}

#[test]
fn escaping_an_existing_edit_leaves_the_original_text() {
    let mut state = app(&["original"]);
    send(&mut state, &A::EditTodo);
    type_text(&mut state, " and more");
    send(&mut state, &A::CancelEdit);

    assert_eq!(visible(&state), ["original"]);
}

#[test]
fn typing_non_ascii_is_kept_whole() {
    let mut state = app(&[]);
    send(&mut state, &A::AddTodo);
    type_text(&mut state, "café 世界 👍");
    send(&mut state, &A::ConfirmEdit);

    assert_eq!(visible(&state), ["café 世界 👍"]);
}

#[test]
fn the_caret_can_move_inside_the_text_being_typed() {
    let mut state = app(&[]);
    send(&mut state, &A::AddTodo);
    type_text(&mut state, "world");
    send(&mut state, &A::Edit(EditKey::Home));
    type_text(&mut state, "hello ");
    send(&mut state, &A::ConfirmEdit);

    assert_eq!(visible(&state), ["hello world"]);
}

// --- visual mode ---

#[test]
fn a_visual_selection_deletes_every_todo_it_covers_in_one_step() {
    let mut state = app(&["a", "b", "c", "d"]);
    send(&mut state, &A::EnterVisual);
    send(&mut state, &A::ExtendVisual(Motion::Down));
    send(&mut state, &A::DeleteSelected);

    assert_eq!(visible(&state), ["c", "d"]);
    assert_eq!(state.main.mode(), MainMode::Normal);

    send(&mut state, &A::Undo);
    assert_eq!(visible(&state), ["a", "b", "c", "d"]);
}

#[test]
fn a_visual_run_reorders_as_a_block() {
    let mut state = app(&["a", "b", "c", "d"]);
    send(&mut state, &A::EnterVisual);
    send(&mut state, &A::ExtendVisual(Motion::Down));
    send(&mut state, &A::Move(Dir::Down));

    assert_eq!(visible(&state), ["c", "a", "b", "d"]);
}

// --- reorder rules ---

#[test]
fn reordering_a_completed_todo_says_so_instead_of_moving_something_else() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::ToggleTodo);
    send(&mut state, &A::Cursor(Motion::End));

    let effects = send(&mut state, &A::Move(Dir::Up));

    assert_eq!(
        toast_text(&effects).as_deref(),
        Some("-- completed todos can't be reordered --")
    );
    assert_eq!(visible(&state), ["b", "a"]);
}

#[test]
fn crossing_a_section_boundary_reassigns_the_project_and_says_which() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    let effects = send(&mut state, &A::Move(Dir::Down));

    assert_eq!(
        toast_text(&effects).as_deref(),
        Some("-- moved to # Work --")
    );
    assert!(state.ws.todos(&Bucket::All).is_empty());
    assert_eq!(state.ws.todos(&Bucket::Project(project)).len(), 2);
    assert_eq!(
        visible(&state),
        ["loose", "owned"],
        "the todo keeps its place on screen"
    );
}

#[test]
fn a_reassigning_move_writes_both_files_and_nothing_else() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    let effects = send(&mut state, &A::Move(Dir::Down));

    let mut written = saves(&effects);
    written.sort_by_key(ToString::to_string);
    assert_eq!(written, [Target::AllTodos, Target::Project(project)]);
}

// --- saving ---

#[test]
fn editing_one_project_writes_only_that_project_file() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    send(&mut state, &A::Cursor(Motion::Down));
    let effects = send(&mut state, &A::ToggleTodo);

    assert_eq!(saves(&effects), [Target::Project(project)]);
}

#[test]
fn moving_the_cursor_writes_nothing() {
    let (mut state, _) = app_with_project(&["loose"], &["owned"]);
    let effects = send(&mut state, &A::Cursor(Motion::Down));

    assert!(saves(&effects).is_empty());
}

#[test]
fn a_bucket_whose_file_failed_to_load_is_never_written_back() {
    let (mut state, project) = app_with_project(&[], &["owned"]);
    state.ws.mark_read_only(Bucket::Project(project));

    let effects = send(&mut state, &A::ToggleTodo);
    assert!(saves(&effects).is_empty());
}

// --- search ---

#[test]
fn searching_filters_live_and_keeps_a_visible_cursor() {
    let mut state = app(&["alpha", "beta", "gamma"]);
    send(&mut state, &A::EnterSearch);
    for ch in "mm".chars() {
        send(&mut state, &A::Search(EditKey::Char(ch)));
    }

    assert_eq!(visible(&state), ["gamma"]);
    assert_eq!(cursor_text(&state).as_deref(), Some("gamma"));
}

#[test]
fn confirming_a_search_moves_to_the_first_match() {
    let mut state = app(&["alpha", "beta", "gamma"]);
    send(&mut state, &A::EnterSearch);
    send(&mut state, &A::Search(EditKey::Char('a')));
    send(&mut state, &A::ConfirmSearch);

    assert_eq!(state.main.mode(), MainMode::SearchNav);
    assert_eq!(cursor_text(&state).as_deref(), Some("alpha"));
}

#[test]
fn cancelling_a_search_restores_the_whole_list() {
    let mut state = app(&["alpha", "beta"]);
    send(&mut state, &A::EnterSearch);
    send(&mut state, &A::Search(EditKey::Char('b')));
    send(&mut state, &A::CancelSearch);

    assert_eq!(visible(&state), ["alpha", "beta"]);
    assert_eq!(state.main.mode(), MainMode::Normal);
}

// --- views and the sidebar ---

#[test]
fn moving_down_the_sidebar_switches_the_view_without_touching_a_file() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    state.pane = Pane::Sidebar;

    let effects = send(&mut state, &A::Sidebar(SidebarAction::Down));

    assert_eq!(state.view, ViewId::Project(project));
    assert_eq!(visible(&state), ["owned"]);
    assert!(saves(&effects).is_empty());
}

#[test]
fn each_view_remembers_its_own_cursor() {
    let (mut state, _) = app_with_project(&["a", "b"], &["x", "y"]);
    state.pane = Pane::Sidebar;

    send(&mut state, &A::Sidebar(SidebarAction::Down));
    state.pane = Pane::Main;
    send(&mut state, &A::Cursor(Motion::Down));
    assert_eq!(cursor_text(&state).as_deref(), Some("y"));

    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::Up));
    assert_eq!(cursor_text(&state).as_deref(), Some("a"));

    send(&mut state, &A::Sidebar(SidebarAction::Down));
    assert_eq!(cursor_text(&state).as_deref(), Some("y"));
}

#[test]
fn creating_a_project_selects_it_and_writes_its_file() {
    let mut state = app(&[]);
    state.pane = Pane::Sidebar;

    send(&mut state, &A::Sidebar(SidebarAction::AddProject));
    assert_eq!(state.sidebar.mode(), SidebarMode::Insert);
    type_sidebar(&mut state, "Errands");
    let effects = send(&mut state, &A::Sidebar(SidebarAction::ConfirmEdit));

    let id = state.ws.projects().as_slice()[0].id.clone();
    assert_eq!(state.ws.projects().len(), 1);
    assert_eq!(state.sidebar_cursor, SidebarCursor::Project(id.clone()));
    assert_eq!(state.view, ViewId::Project(id.clone()));
    assert_eq!(saves(&effects), [Target::Project(id)]);
}

#[test]
fn a_project_holding_unfinished_work_asks_before_it_is_deleted() {
    let (mut state, project) = app_with_project(&[], &["still open"]);
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(project.clone());

    send(&mut state, &A::Sidebar(SidebarAction::Delete));
    assert!(matches!(state.sidebar, SidebarState::ConfirmDelete(_)));
    assert_eq!(state.ws.projects().len(), 1);

    send(&mut state, &A::Sidebar(SidebarAction::CancelDelete));
    assert_eq!(state.ws.projects().len(), 1);

    send(&mut state, &A::Sidebar(SidebarAction::Delete));
    let effects = send(&mut state, &A::Sidebar(SidebarAction::ConfirmDelete));

    assert_eq!(state.ws.projects().len(), 0);
    assert!(effects.contains(&Effect::DeleteProject(project)));
    assert_eq!(state.view, ViewId::All);
}

#[test]
fn a_project_with_nothing_outstanding_is_deleted_without_a_prompt() {
    let (mut state, project) = app_with_project(&[], &[]);
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(project.clone());

    let effects = send(&mut state, &A::Sidebar(SidebarAction::Delete));

    assert_eq!(state.ws.projects().len(), 0);
    assert!(effects.contains(&Effect::DeleteProject(project)));
}

#[test]
fn deleting_a_project_cascades_to_its_children() {
    let mut ws = Workspace::default();
    let parent = ws.add_project("Parent".into(), None);
    let child = ws.add_project("Child".into(), Some(parent.clone()));
    let mut state = AppState::new(ws, geometry());
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(parent.clone());

    let effects = send(&mut state, &A::Sidebar(SidebarAction::Delete));

    assert_eq!(state.ws.projects().len(), 0);
    assert!(effects.contains(&Effect::DeleteProject(parent)));
    assert!(effects.contains(&Effect::DeleteProject(child)));
}

#[test]
fn undoing_a_project_delete_brings_back_its_todos_and_rewrites_the_file() {
    let (mut state, project) = app_with_project(&[], &["important"]);
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(project.clone());
    send(&mut state, &A::Sidebar(SidebarAction::Delete));
    send(&mut state, &A::Sidebar(SidebarAction::ConfirmDelete));

    let effects = send(&mut state, &A::Undo);

    assert_eq!(state.ws.projects().len(), 1);
    assert_eq!(
        state.ws.todos(&Bucket::Project(project.clone()))[0].text,
        "important"
    );
    assert!(saves(&effects).contains(&Target::Project(project)));
}

#[test]
fn the_all_todos_row_refuses_project_operations_out_loud() {
    let mut state = app(&[]);
    state.pane = Pane::Sidebar;

    for action in [
        SidebarAction::Rename,
        SidebarAction::Delete,
        SidebarAction::Move(Dir::Down),
        SidebarAction::AddSubproject,
    ] {
        let effects = send(&mut state, &A::Sidebar(action));
        assert!(
            toast_text(&effects).is_some(),
            "{action:?} should explain itself rather than doing nothing"
        );
    }
}

#[test]
fn a_subproject_cannot_have_a_subproject_of_its_own() {
    let mut ws = Workspace::default();
    let parent = ws.add_project("Parent".into(), None);
    let child = ws.add_project("Child".into(), Some(parent));
    let mut state = AppState::new(ws, geometry());
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(child);

    let effects = send(&mut state, &A::Sidebar(SidebarAction::AddSubproject));

    assert_eq!(
        toast_text(&effects).as_deref(),
        Some("-- only two levels of projects --")
    );
    assert_eq!(state.ws.projects().len(), 2);
}

// --- undo ---

#[test]
fn undo_and_redo_walk_the_same_path_in_both_directions() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    assert_eq!(visible(&state), ["b"]);

    send(&mut state, &A::Undo);
    assert_eq!(visible(&state), ["a", "b"]);

    send(&mut state, &A::Redo);
    assert_eq!(visible(&state), ["b"]);
}

#[test]
fn undo_puts_the_cursor_back_on_what_it_changed() {
    let mut state = app(&["a", "b", "c"]);
    send(&mut state, &A::Cursor(Motion::Down));
    send(&mut state, &A::Move(Dir::Down));
    assert_eq!(visible(&state), ["a", "c", "b"]);

    send(&mut state, &A::Undo);
    assert_eq!(visible(&state), ["a", "b", "c"]);
    assert_eq!(cursor_text(&state).as_deref(), Some("b"));
}

#[test]
fn undo_returns_to_the_view_the_change_happened_in() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::Down));
    state.pane = Pane::Main;
    send(&mut state, &A::DeleteTodo);

    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::Up));
    assert_eq!(state.view, ViewId::All);

    send(&mut state, &A::Undo);
    assert_eq!(state.view, ViewId::Project(project));
    assert_eq!(visible(&state), ["owned"]);
}

#[test]
fn undo_with_nothing_left_says_so_rather_than_doing_nothing() {
    let mut state = app(&["a"]);
    let effects = send(&mut state, &A::Undo);

    assert_eq!(
        toast_text(&effects).as_deref(),
        Some("-- already at the oldest change --")
    );
}

#[test]
fn navigation_is_not_undoable() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::Cursor(Motion::Down));
    send(&mut state, &A::EnterVisual);
    send(&mut state, &A::ExitVisual);

    let effects = send(&mut state, &A::Undo);
    assert_eq!(
        toast_text(&effects).as_deref(),
        Some("-- already at the oldest change --")
    );
}

// --- quitting ---

#[test]
fn q_quits_straight_away_when_nothing_has_failed() {
    let mut state = app(&["a"]);
    assert_eq!(send(&mut state, &A::Quit), [Effect::Quit]);
}

/// The guard follows the failed write, not the message about it: a later toast replaces
/// the warning on screen, and that must not make an unwritten file look written.
#[test]
fn q_asks_once_more_while_a_save_has_failed() {
    use doer_core::store::{StoreError, Target};

    let mut state = app(&["a"]);
    state.save_failed(
        Some(Target::AllTodos),
        &StoreError::RefusedReadOnly {
            target: Target::AllTodos,
        },
    );
    state.toast = Some(doer_core::Toast {
        text: "-- undo --".into(),
        level: ToastLevel::Info,
        ttl_ms: Some(2500),
        seq: 9,
    });

    let first = send(&mut state, &A::Quit);
    assert!(!first.contains(&Effect::Quit));
    assert!(toast_text(&first).is_some());

    assert!(send(&mut state, &A::Quit).contains(&Effect::Quit));
}

#[test]
fn ctrl_c_quits_even_with_an_error_standing() {
    let mut state = app(&["a"]);
    state.toast = Some(doer_core::Toast {
        text: "save failed".into(),
        level: ToastLevel::Error,
        ttl_ms: None,
        seq: 1,
    });

    assert!(send(&mut state, &A::ForceQuit).contains(&Effect::Quit));
}

// --- toasts ---

#[test]
fn a_toast_expiry_for_a_superseded_message_is_ignored() {
    let (mut state, _) = app_with_project(&["loose"], &["owned"]);
    let first = send(&mut state, &A::Move(Dir::Down));
    let Some(Effect::Toast(older)) = first
        .iter()
        .find(|e| matches!(e, Effect::Toast(_)))
        .cloned()
    else {
        panic!("expected the reassignment toast");
    };

    send(&mut state, &A::ToggleTodo);
    send(&mut state, &A::Cursor(Motion::End));
    let second = send(&mut state, &A::Move(Dir::Up));
    assert_eq!(
        toast_text(&second).as_deref(),
        Some("-- completed todos can't be reordered --")
    );

    send(&mut state, &A::ToastExpire(older.seq));
    assert!(
        state.toast.is_some(),
        "the newer toast must survive the older one's timer"
    );

    let current = state.toast.as_ref().expect("toast").seq;
    send(&mut state, &A::ToastExpire(current));
    assert!(state.toast.is_none());
}

// --- layout coupling ---

#[test]
fn resizing_re_clamps_the_scroll_offset() {
    let texts: Vec<String> = (0..60).map(|i| format!("todo {i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut state = app(&refs);

    send(&mut state, &A::Cursor(Motion::End));
    let tall = state.scroll;
    assert!(tall > 0);

    send(&mut state, &A::Resize(100, 200));
    assert_eq!(
        state.scroll, 0,
        "a terminal taller than the list cannot be scrolled"
    );
}

#[test]
fn the_cursor_row_stays_inside_the_viewport_however_the_list_wraps() {
    let long = "a sentence long enough that it certainly wraps across several rows in the pane";
    let texts: Vec<String> = (0..40).map(|i| format!("{i} {long}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut state = app(&refs);

    for _ in 0..40 {
        send(&mut state, &A::Cursor(Motion::Down));
        let layout = state.layout(NOW);
        let span = state
            .cursor
            .as_ref()
            .and_then(|id| layout.span(id))
            .expect("the cursor is always on a rendered row");
        let viewport = state.geo.viewport_height();
        assert!(
            span.start >= state.scroll && span.start < state.scroll + viewport,
            "cursor row {span:?} outside viewport at offset {}",
            state.scroll
        );
    }
}

#[test]
fn every_action_leaves_the_workspace_valid_and_the_cursor_resolvable() {
    let (mut state, _) = app_with_project(&["a", "b"], &["x"]);
    let script = [
        A::AddTodo,
        A::Edit(EditKey::Char('z')),
        A::ConfirmEdit,
        A::Cursor(Motion::Down),
        A::ToggleTodo,
        A::Move(Dir::Down),
        A::EnterVisual,
        A::ExtendVisual(Motion::Down),
        A::DeleteSelected,
        A::EnterSearch,
        A::Search(EditKey::Char('a')),
        A::CancelSearch,
        A::Undo,
        A::Undo,
        A::Redo,
        A::Cursor(Motion::End),
        A::DeleteTodo,
    ];

    for action in script {
        send(&mut state, &action);
        assert!(state.ws.is_valid());
        let ids: Vec<TodoId> = state.order_ids();
        match &state.cursor {
            Some(cursor) => assert!(ids.contains(cursor)),
            None => assert!(ids.is_empty()),
        }
    }
}

// --- properties ---

/// Everything about the workspace that a user could notice, in a comparable form.
fn fingerprint(ws: &Workspace) -> String {
    ws.buckets_in_display_order()
        .iter()
        .map(|bucket| {
            let todos: Vec<String> = ws
                .todos(bucket)
                .iter()
                .map(|t| format!("{}:{}:{}", t.id, t.text, t.done))
                .collect();
            format!("{bucket:?}=[{}]", todos.join(","))
        })
        .collect::<Vec<_>>()
        .join(";")
}

proptest::proptest! {
    #[test]
    fn undoing_every_change_and_redoing_them_all_returns_the_same_workspace(
        script in proptest::collection::vec(0u8..9, 1..40)
    ) {
        let (mut state, _) = app_with_project(&["a", "b", "c"], &["x", "y"]);

        let mut mutations = 0usize;
        for step in script {
            let action = match step {
                0 => A::Cursor(Motion::Down),
                1 => A::Cursor(Motion::Up),
                2 => A::ToggleTodo,
                3 => A::DeleteTodo,
                4 => A::Move(Dir::Down),
                5 => A::Move(Dir::Up),
                6 => A::EnterVisual,
                7 => A::ExtendVisual(Motion::Down),
                _ => A::DeleteSelected,
            };
            let before = fingerprint(&state.ws);
            send(&mut state, &action);
            if fingerprint(&state.ws) != before {
                mutations += 1;
            }
        }

        let after = fingerprint(&state.ws);
        for _ in 0..mutations {
            send(&mut state, &A::Undo);
        }
        for _ in 0..mutations {
            send(&mut state, &A::Redo);
        }

        proptest::prop_assert_eq!(fingerprint(&state.ws), after);
    }

    #[test]
    fn the_cursor_is_always_either_absent_or_on_a_visible_todo(
        script in proptest::collection::vec(0u8..7, 1..30)
    ) {
        let (mut state, _) = app_with_project(&["a", "b"], &["x"]);

        for step in script {
            let action = match step {
                0 => A::Cursor(Motion::Down),
                1 => A::Cursor(Motion::End),
                2 => A::ToggleTodo,
                3 => A::DeleteTodo,
                4 => A::Move(Dir::Down),
                5 => A::Undo,
                _ => A::Redo,
            };
            send(&mut state, &action);

            let ids = state.order_ids();
            match &state.cursor {
                Some(cursor) => proptest::prop_assert!(ids.contains(cursor)),
                None => proptest::prop_assert!(ids.is_empty()),
            }
            proptest::prop_assert!(state.ws.is_valid());
        }
    }
}

// --- store integration ---

#[test]
fn a_file_that_failed_to_load_leaves_a_standing_error_and_is_never_written() {
    use doer_core::store::{Loaded, Problem, ProjectFile, StoreSnapshot};

    let project = doer_core::project::Project::new("Work", 0, None);
    let id = project.id.clone();
    let snapshot = StoreSnapshot {
        all_todos: vec![Todo::new("loose", NOW)],
        projects: vec![ProjectFile::new(&project, vec![Todo::new("owned", NOW)])],
        read_only: vec![Target::Project(id.clone())],
    };
    let problems = vec![Problem::Corrupt {
        path: format!("{id}.json").into(),
        detail: "trailing comma".into(),
    }];

    let (mut state, startup) = AppState::from_loaded(Loaded::new(snapshot, problems), geometry());

    let toast = startup
        .iter()
        .find_map(|e| match e {
            Effect::Toast(toast) => Some(toast.clone()),
            _ => None,
        })
        .expect("the load problem is reported");
    assert_eq!(toast.level, ToastLevel::Error);
    assert_eq!(toast.ttl_ms, None, "a save-blocking error must not fade");
    assert!(state.ws.is_read_only(&Bucket::Project(id.clone())));

    send(&mut state, &A::Cursor(Motion::Down));
    let effects = send(&mut state, &A::ToggleTodo);
    assert!(
        saves(&effects).is_empty(),
        "a damaged file must not be overwritten with our partial reading of it"
    );
}

#[test]
fn a_failed_save_is_reported_and_makes_q_ask_twice() {
    use doer_core::store::{StoreError, Target};

    let mut state = app(&["a"]);
    let effect = state.save_failed(
        Some(Target::AllTodos),
        &StoreError::RefusedReadOnly {
            target: Target::AllTodos,
        },
    );

    let Effect::Toast(toast) = effect else {
        panic!("expected a toast");
    };
    assert_eq!(toast.level, ToastLevel::Error);
    assert!(toast.text.contains("all-todos.json"));

    assert!(!send(&mut state, &A::Quit).contains(&Effect::Quit));
    assert!(send(&mut state, &A::Quit).contains(&Effect::Quit));
}

#[test]
fn a_later_successful_save_of_the_same_file_clears_the_error() {
    use doer_core::store::{StoreError, Target};

    let mut state = app(&["a"]);
    state.save_failed(
        Some(Target::AllTodos),
        &StoreError::RefusedReadOnly {
            target: Target::AllTodos,
        },
    );
    state.save_succeeded(&Target::AllTodos);

    assert!(state.toast.is_none());
    assert!(!state.has_failed_saves());
    assert!(send(&mut state, &A::Quit).contains(&Effect::Quit));
}

/// A write to one file says nothing about another. Clearing the warning on it would
/// retract it while the stale file is still on disk, and let `q` exit without asking.
#[test]
fn a_successful_save_of_another_file_leaves_the_error_standing() {
    use doer_core::id::ProjectId;
    use doer_core::store::{StoreError, Target};

    let mut state = app(&["a"]);
    state.save_failed(
        Some(Target::AllTodos),
        &StoreError::RefusedReadOnly {
            target: Target::AllTodos,
        },
    );
    state.save_succeeded(&Target::Project(ProjectId::from("0123456789abcdef")));

    assert!(state.toast.is_some(), "the warning must survive");
    assert!(state.has_failed_saves());
    assert!(
        !send(&mut state, &A::Quit).contains(&Effect::Quit),
        "q must still ask"
    );
}

#[test]
fn the_mode_bar_counts_the_view_and_ignores_the_search_filter() {
    let (mut state, _) = app_with_project(&["loose"], &["owned"]);
    send(&mut state, &A::ToggleTodo);
    assert_eq!(state.counts(), (1, 2));

    send(&mut state, &A::EnterSearch);
    send(&mut state, &A::Search(EditKey::Char('z')));
    assert!(visible(&state).is_empty());
    assert_eq!(state.counts(), (1, 2));

    send(&mut state, &A::CancelSearch);
    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::Down));
    assert_eq!(state.counts(), (0, 1), "counts follow the current view");
}

// --- undo must not leave orphaned files ---

fn deletes(effects: &[Effect]) -> Vec<doer_core::ProjectId> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::DeleteProject(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn undoing_a_project_creation_deletes_the_file_it_wrote() {
    let mut state = app(&[]);
    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::AddProject));
    type_sidebar(&mut state, "Errands");
    let created = send(&mut state, &A::Sidebar(SidebarAction::ConfirmEdit));
    let id = state.ws.projects().as_slice()[0].id.clone();
    assert_eq!(saves(&created), [Target::Project(id.clone())]);

    let undone = send(&mut state, &A::Undo);

    assert_eq!(state.ws.projects().len(), 0);
    assert!(
        !saves(&undone).contains(&Target::Project(id.clone())),
        "a project that no longer exists must not also be queued for writing"
    );
    assert_eq!(
        deletes(&undone),
        [id],
        "otherwise the project reappears on the next launch"
    );
}

#[test]
fn redoing_a_project_delete_deletes_the_file_again() {
    let (mut state, project) = app_with_project(&[], &["work"]);
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(project.clone());
    send(&mut state, &A::Sidebar(SidebarAction::Delete));
    send(&mut state, &A::Sidebar(SidebarAction::ConfirmDelete));
    send(&mut state, &A::Undo);

    let redone = send(&mut state, &A::Redo);

    assert_eq!(state.ws.projects().len(), 0);
    assert_eq!(deletes(&redone), [project]);
}

#[test]
fn undoing_a_project_delete_still_rewrites_the_file_and_deletes_nothing() {
    let (mut state, project) = app_with_project(&[], &["important"]);
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(project.clone());
    send(&mut state, &A::Sidebar(SidebarAction::Delete));
    send(&mut state, &A::Sidebar(SidebarAction::ConfirmDelete));

    let undone = send(&mut state, &A::Undo);

    assert!(saves(&undone).contains(&Target::Project(project)));
    assert!(deletes(&undone).is_empty());
}

#[test]
fn a_project_that_survives_an_undo_is_not_deleted() {
    let (mut state, kept) = app_with_project(&["loose"], &[]);
    send(&mut state, &A::DeleteTodo);

    let undone = send(&mut state, &A::Undo);

    assert!(deletes(&undone).is_empty(), "only the todo changed");
    assert!(saves(&undone).contains(&Target::Project(kept)));
}

// --- a refused reorder must change nothing at all ---

#[test]
fn a_sidebar_reorder_at_the_end_of_a_level_leaves_the_redo_branch_alone() {
    let mut ws = Workspace::default();
    ws.add_project("Only".into(), None);
    let mut state = AppState::new(ws, geometry());
    let id = state.ws.projects().as_slice()[0].id.clone();

    send(&mut state, &A::AddTodo);
    type_text(&mut state, "a todo");
    send(&mut state, &A::ConfirmEdit);
    send(&mut state, &A::Undo);
    assert!(state.undo.can_redo());

    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(id);
    let refused = send(&mut state, &A::Sidebar(SidebarAction::Move(Dir::Down)));

    assert!(
        saves(&refused).is_empty(),
        "nothing moved, so nothing to save"
    );
    assert!(
        state.undo.can_redo(),
        "a keypress that did nothing must not discard the redo branch"
    );
    send(&mut state, &A::Redo);
    assert_eq!(visible(&state), ["a todo"]);
}

#[test]
fn a_refused_todo_reorder_also_leaves_the_redo_branch_alone() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    send(&mut state, &A::Undo);
    assert!(state.undo.can_redo());

    send(&mut state, &A::Move(Dir::Up));

    assert!(state.undo.can_redo());
}

#[test]
fn reordering_projects_still_works_when_there_is_a_sibling() {
    let mut ws = Workspace::default();
    ws.add_project("First".into(), None);
    ws.add_project("Second".into(), None);
    let mut state = AppState::new(ws, geometry());
    let first = state.ws.projects().flat_ordered()[0].id.clone();
    state.pane = Pane::Sidebar;
    state.sidebar_cursor = SidebarCursor::Project(first.clone());

    let effects = send(&mut state, &A::Sidebar(SidebarAction::Move(Dir::Down)));

    assert!(!saves(&effects).is_empty());
    assert_eq!(state.ws.projects().flat_ordered()[1].id, first);
}

#[test]
fn toggling_the_sidebar_keeps_the_layout_and_the_keymap_agreeing() {
    let mut state = app(&["a"]);
    assert!(state.sidebar_open());
    assert!(state.geo.sidebar_open);

    send(&mut state, &A::ToggleSidebar);

    assert!(!state.sidebar_open());
    assert!(!state.geo.sidebar_open);
    assert!(!state.input_context().sidebar_open);
}

// --- a file we could not read is never written, and never silently ---

fn warnings(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Toast(toast) if toast.level == ToastLevel::Warning => Some(toast.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn editing_a_locked_bucket_is_refused_and_says_so() {
    let mut state = app(&["keep me"]);
    state.ws.mark_read_only(Bucket::All);

    let effects = send(&mut state, &A::ToggleTodo);

    assert_eq!(
        warnings(&effects),
        ["-- ungrouped todos couldn't be loaded; they're locked --"]
    );
    assert!(saves(&effects).is_empty());
    assert!(
        !state.ws.todos(&Bucket::All)[0].done,
        "the refusal must leave the todo exactly as it was"
    );
}

#[test]
fn a_refused_edit_leaves_nothing_on_the_undo_stack() {
    let mut state = app(&["keep me"]);
    state.ws.mark_read_only(Bucket::All);
    let depth = state.undo.depth();

    send(&mut state, &A::ToggleTodo);
    send(&mut state, &A::DeleteTodo);

    assert_eq!(state.undo.depth(), depth, "a refusal is not a change");
    assert_eq!(visible(&state), ["keep me"]);
}

#[test]
fn adding_a_todo_to_a_locked_bucket_is_refused_without_leaving_a_stub_behind() {
    let mut state = app(&["existing"]);
    state.ws.mark_read_only(Bucket::All);

    let effects = send(&mut state, &A::AddTodo);

    assert_eq!(warnings(&effects).len(), 1);
    assert_eq!(
        state.main.mode(),
        MainMode::Normal,
        "insert mode not entered"
    );
    assert_eq!(visible(&state), ["existing"]);
    assert_eq!(state.undo.depth(), 0);
}

#[test]
fn reordering_a_todo_into_a_locked_project_is_refused() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    state.ws.mark_read_only(Bucket::Project(project.clone()));

    let effects = send(&mut state, &A::Move(Dir::Down));

    assert_eq!(warnings(&effects).len(), 1);
    assert_eq!(
        state.ws.todos(&Bucket::All).len(),
        1,
        "the todo stayed where it was"
    );
    assert_eq!(state.ws.todos(&Bucket::Project(project)).len(), 1);
}

#[test]
fn an_unrelated_edit_still_works_while_another_bucket_is_locked() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    state.ws.mark_read_only(Bucket::All);
    state.view = ViewId::Project(project.clone());
    state.cursor = Some(
        state.ws.todos(&Bucket::Project(project.clone()))[0]
            .id
            .clone(),
    );

    let effects = send(&mut state, &A::ToggleTodo);

    assert!(warnings(&effects).is_empty());
    assert_eq!(saves(&effects), [Target::Project(project.clone())]);
    assert!(state.ws.todos(&Bucket::Project(project))[0].done);
}

#[test]
fn undo_is_refused_only_when_it_would_rewrite_the_locked_bucket() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    assert_eq!(visible(&state), ["b"]);

    // Locked after the change, so the undo stack holds an entry that would rewrite it.
    state.ws.mark_read_only(Bucket::All);
    let effects = send(&mut state, &A::Undo);

    assert_eq!(warnings(&effects).len(), 1);
    assert_eq!(visible(&state), ["b"], "the refused undo changed nothing");
    assert!(state.undo.can_undo(), "and the entry is still there");
}

#[test]
fn undo_of_a_change_elsewhere_is_allowed_while_a_bucket_is_locked() {
    let (mut state, project) = app_with_project(&["loose"], &["owned"]);
    state.view = ViewId::Project(project.clone());
    state.cursor = Some(
        state.ws.todos(&Bucket::Project(project.clone()))[0]
            .id
            .clone(),
    );
    send(&mut state, &A::DeleteTodo);
    assert!(state.ws.todos(&Bucket::Project(project.clone())).is_empty());

    state.ws.mark_read_only(Bucket::All);
    let effects = send(&mut state, &A::Undo);

    assert!(
        warnings(&effects).is_empty(),
        "one damaged file must not disable undo everywhere else"
    );
    assert_eq!(state.ws.todos(&Bucket::Project(project)).len(), 1);
}

// --- undo says what it undid ---

fn infos(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Toast(toast) if toast.level == ToastLevel::Info => Some(toast.text.clone()),
            _ => None,
        })
        .collect()
}

fn undo_message(state: &mut AppState) -> String {
    infos(&send(state, &A::Undo)).join(",")
}

#[test]
fn undo_names_the_change_it_reverses() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    assert_eq!(undo_message(&mut state), "-- undo: delete --");

    send(&mut state, &A::ToggleTodo);
    assert_eq!(undo_message(&mut state), "-- undo: toggle --");

    send(&mut state, &A::Move(Dir::Down));
    assert_eq!(undo_message(&mut state), "-- undo: reorder --");

    send(&mut state, &A::EditTodo);
    type_text(&mut state, " more");
    send(&mut state, &A::ConfirmEdit);
    assert_eq!(undo_message(&mut state), "-- undo: edit --");

    send(&mut state, &A::AddTodo);
    type_text(&mut state, "fresh");
    send(&mut state, &A::ConfirmEdit);
    assert_eq!(undo_message(&mut state), "-- undo: add --");
}

#[test]
fn undo_of_a_cross_section_reassignment_says_it_moved() {
    let (mut state, _) = app_with_project(&["loose"], &["owned"]);
    send(&mut state, &A::Move(Dir::Down));

    assert_eq!(undo_message(&mut state), "-- undo: move --");
}

#[test]
fn undo_of_a_project_change_says_project() {
    let mut state = app(&[]);
    state.pane = Pane::Sidebar;
    send(&mut state, &A::Sidebar(SidebarAction::AddProject));
    type_sidebar(&mut state, "Errands");
    send(&mut state, &A::Sidebar(SidebarAction::ConfirmEdit));

    assert_eq!(undo_message(&mut state), "-- undo: project --");
}

#[test]
fn redo_uses_its_own_verb() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    send(&mut state, &A::Undo);

    assert_eq!(
        infos(&send(&mut state, &A::Redo)),
        ["-- redo: delete --"],
        "redoing a delete deletes again, so the name describes the change either way"
    );
}

#[test]
fn a_refused_undo_reports_the_lock_rather_than_a_success() {
    let mut state = app(&["a", "b"]);
    send(&mut state, &A::DeleteTodo);
    state.ws.mark_read_only(Bucket::All);

    let effects = send(&mut state, &A::Undo);

    assert!(infos(&effects).is_empty(), "nothing was undone");
    assert_eq!(warnings(&effects).len(), 1);
}
