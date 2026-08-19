//! The reducer: every state change in the app happens here, and nothing here
//! touches the outside world.
//!
//! `reduce` returns [`Effect`]s describing the IO the shell should perform. Saving,
//! quitting and toast timers are all descriptions, so a test can drive the entire
//! application by feeding it actions and reading state back.

use crate::action::{Action, Dir, EditKey, Motion, ResolveCursor, SidebarAction};
use crate::dirty::{DirtySet, Touched, target_of};
use crate::display::{DisplayList, ViewId};
use crate::id::{ProjectId, TodoId};
use crate::layout::{Geometry, Layout, LayoutHints, adjust_scroll, clamp_scroll};
use crate::mode::{Focus, MainMode, SidebarMode};
use crate::project::Project;
use crate::reorder::{self, Blocked, Reorder};
use crate::store::{Loaded, ProjectFile, StoreError, StoreSnapshot, Target};
use crate::text::TextInput;
use crate::todo::Todo;
use crate::undo::{Snapshot, UndoStack};
use crate::workspace::{Bucket, Workspace};

pub const TOAST_TTL_MS: u64 = 2_500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub level: ToastLevel,
    /// `None` never expires on its own. A failed save stays on screen until a later
    /// save succeeds, because a message the user missed is a message that lied.
    pub ttl_ms: Option<u64>,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Save(Target),
    DeleteProject(ProjectId),
    Toast(Toast),
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Main,
    Sidebar,
}

/// The sidebar cursor is an identity, not a row number, so a cascading project
/// delete cannot leave it pointing at whatever slid into that slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SidebarCursor {
    #[default]
    All,
    Project(ProjectId),
}

#[derive(Clone, Debug)]
pub enum EditTarget {
    /// A todo already inserted into the list so it renders in place while being
    /// typed. Abandoning the edit removes it again.
    New(TodoId),
    Existing {
        id: TodoId,
        original: String,
    },
}

#[derive(Clone, Debug)]
pub struct Editing {
    pub target: EditTarget,
    pub input: TextInput,
}

#[derive(Clone, Debug, Default)]
pub enum MainState {
    #[default]
    Normal,
    Insert(Editing),
    Visual {
        anchor: TodoId,
    },
    Search(TextInput),
    SearchNav {
        query: String,
    },
}

impl MainState {
    #[must_use]
    pub fn mode(&self) -> MainMode {
        match self {
            Self::Normal => MainMode::Normal,
            Self::Insert(_) => MainMode::Insert,
            Self::Visual { .. } => MainMode::Visual,
            Self::Search(_) => MainMode::Search,
            Self::SearchNav { .. } => MainMode::SearchNav,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ProjectEdit {
    NewTopLevel,
    NewChild(ProjectId),
    Rename(ProjectId),
}

#[derive(Clone, Debug, Default)]
pub enum SidebarState {
    #[default]
    Normal,
    Insert {
        target: ProjectEdit,
        input: TextInput,
    },
    ConfirmDelete(ProjectId),
}

impl SidebarState {
    #[must_use]
    pub fn mode(&self) -> SidebarMode {
        match self {
            Self::Normal => SidebarMode::Normal,
            Self::Insert { .. } => SidebarMode::Insert,
            Self::ConfirmDelete(_) => SidebarMode::ConfirmDelete,
        }
    }
}

/// Cursor, scroll and search survive a trip to another view and back.
#[derive(Clone, Debug, Default)]
struct ViewState {
    cursor: Option<TodoId>,
    cursor_hint: usize,
    scroll: usize,
    search: String,
}

pub struct AppState {
    pub ws: Workspace,
    pub view: ViewId,
    views: Vec<(ViewId, ViewState)>,

    pub pane: Pane,
    pub main: MainState,
    pub sidebar: SidebarState,
    pub sidebar_cursor: SidebarCursor,

    pub cursor: Option<TodoId>,
    /// Where the cursor was last seen in display order. Only consulted when the
    /// todo it pointed at no longer exists.
    cursor_hint: usize,
    pub scroll: usize,

    pub geo: Geometry,
    pub help: bool,
    pub toast: Option<Toast>,
    toast_seq: u64,

    pub undo: UndoStack,
    dirty: DirtySet,
    /// Set while a new todo is being typed: the insert and the text that follows it
    /// collapse into a single undo entry, and abandoning the edit undoes both.
    session: Option<Vec<Target>>,
    quit_armed: bool,
}

impl AppState {
    #[must_use]
    pub fn new(ws: Workspace, geo: Geometry) -> Self {
        let mut state = Self {
            ws,
            view: ViewId::All,
            views: Vec::new(),
            pane: Pane::Main,
            main: MainState::Normal,
            sidebar: SidebarState::Normal,
            sidebar_cursor: SidebarCursor::All,
            cursor: None,
            cursor_hint: 0,
            scroll: 0,
            geo,
            help: false,
            toast: None,
            toast_seq: 0,
            undo: UndoStack::default(),
            dirty: DirtySet::default(),
            session: None,
            quit_armed: false,
        };
        state.cursor = state.order_ids().first().cloned();
        state
    }

    /// Builds the state a freshly loaded store implies, including the toast that
    /// reports anything the load could not make sense of.
    #[must_use]
    pub fn from_loaded(loaded: Loaded<StoreSnapshot>, geo: Geometry) -> (Self, Vec<Effect>) {
        let (snapshot, problems) = loaded.into_parts();
        let mut ws = workspace_from(&snapshot);
        for target in &snapshot.read_only {
            ws.mark_read_only(match target {
                Target::AllTodos => Bucket::All,
                Target::Project(id) => Bucket::Project(id.clone()),
            });
        }

        let mut state = Self::new(ws, geo);
        // Errors last, so the one that does not auto-expire is the one left standing.
        let mut lines = crate::store::toasts(&problems);
        lines.sort_by_key(|(_, severity)| *severity == crate::store::Severity::Error);
        let effects = lines
            .into_iter()
            .map(|(text, severity)| Effect::Toast(state.new_toast(text, toast_level(severity))))
            .collect();
        (state, effects)
    }

    /// The shell reports a failed write back through here so the message, the quit
    /// guard and the still-dirty target all stay in one place.
    pub fn save_failed(&mut self, error: &StoreError) -> Effect {
        Effect::Toast(self.new_toast(error.toast(), ToastLevel::Error))
    }

    /// Clears a standing save error once a later write gets through.
    pub fn save_succeeded(&mut self) {
        if self.has_error_toast() {
            self.toast = None;
        }
    }

    /// Kept on `geo` alone, because that is what the layout reads; a second copy on
    /// `AppState` would be one more thing to hold in step by hand.
    #[must_use]
    pub fn sidebar_open(&self) -> bool {
        self.geo.sidebar_open
    }

    #[must_use]
    pub fn focus(&self) -> Focus {
        match self.pane {
            Pane::Main => Focus::Main(self.main.mode()),
            Pane::Sidebar => Focus::Sidebar(self.sidebar.mode()),
        }
    }

    #[must_use]
    pub fn input_context(&self) -> crate::input::InputContext {
        crate::input::InputContext {
            focus: self.focus(),
            sidebar_open: self.sidebar_open(),
            help: self.help,
        }
    }

    /// The live search query, which is also what filters the display list.
    #[must_use]
    pub fn filter(&self) -> Option<&str> {
        match &self.main {
            MainState::Search(input) => Some(input.text()),
            MainState::SearchNav { query } => Some(query.as_str()),
            _ => None,
        }
    }

    /// `(done, total)` for the mode bar, over the current view and ignoring any
    /// active search: the count describes the list, not the filter.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let all = DisplayList::build(&self.ws, &self.view, None);
        (all.completed.len(), all.len())
    }

    #[must_use]
    pub fn display(&self) -> DisplayList<'_> {
        DisplayList::build(&self.ws, &self.view, self.filter())
    }

    #[must_use]
    pub fn layout_hints(&self) -> LayoutHints<'_> {
        LayoutHints {
            editing: match &self.main {
                MainState::Insert(editing) => Some((
                    editing.id().clone(),
                    editing.input.text(),
                    editing.input.caret_col(),
                )),
                _ => None,
            },
        }
    }

    #[must_use]
    pub fn layout(&self, now: i64) -> Layout {
        Layout::build(
            &self.ws,
            &self.display(),
            &self.view,
            &self.geo,
            &self.layout_hints(),
            now,
        )
    }

    #[must_use]
    pub fn selection(&self) -> Option<std::ops::Range<usize>> {
        let cursor = self.cursor_index()?;
        match &self.main {
            MainState::Visual { anchor } => {
                let ids = self.order_ids();
                let anchor = ids.iter().position(|id| id == anchor)?;
                Some(anchor.min(cursor)..anchor.max(cursor) + 1)
            }
            _ => Some(cursor..cursor + 1),
        }
    }

    #[must_use]
    pub fn selected_ids(&self) -> Vec<TodoId> {
        let ids = self.order_ids();
        self.selection()
            .map(|range| {
                range
                    .filter_map(|index| ids.get(index).cloned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn order_ids(&self) -> Vec<TodoId> {
        self.display()
            .order()
            .iter()
            .map(|todo| todo.id.clone())
            .collect()
    }

    #[must_use]
    pub fn cursor_index(&self) -> Option<usize> {
        let cursor = self.cursor.as_ref()?;
        self.order_ids().iter().position(|id| id == cursor)
    }

    /// The single place the workspace may be mutated. Snapshot, apply, check, record.
    fn mutate<R>(&mut self, change: impl FnOnce(&mut Workspace) -> (R, Touched)) -> R {
        if self.session.is_none() {
            self.undo.push(self.snapshot());
        }
        let (result, touched) = change(&mut self.ws);
        debug_assert!(self.ws.is_valid(), "a mutation left the workspace invalid");

        if let Some(session) = self.session.as_mut() {
            for target in &touched.saves {
                if !session.contains(target) {
                    session.push(target.clone());
                }
            }
        }
        self.dirty.absorb(touched);
        result
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            ws: self.ws.clone(),
            view: self.view.clone(),
            cursor: self.cursor.clone(),
            cursor_hint: self.cursor_hint,
            sidebar_cursor: self.sidebar_cursor.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        let mut touched = Touched::saving_all(
            snapshot
                .ws
                .buckets_in_display_order()
                .iter()
                .map(target_of)
                .collect(),
        );
        // A project that exists now but not in the snapshot has to lose its file too,
        // or undoing a create leaves it on disk to reappear on the next launch.
        let kept = project_ids(&snapshot.ws);
        touched.deletes = project_ids(&self.ws)
            .into_iter()
            .filter(|id| !kept.contains(id))
            .collect();
        self.dirty.absorb(touched);

        self.ws = snapshot.ws;
        self.view = snapshot.view;
        self.cursor = snapshot.cursor;
        self.cursor_hint = snapshot.cursor_hint;
        self.sidebar_cursor = snapshot.sidebar_cursor;
        self.main = MainState::Normal;
        self.sidebar = SidebarState::Normal;
    }

    fn begin_session(&mut self) {
        self.undo.push(self.snapshot());
        self.session = Some(Vec::new());
    }

    fn end_session(&mut self) {
        self.session = None;
    }

    /// Rolls the workspace back to where the edit began, so an abandoned `a` leaves
    /// nothing behind and costs no undo step.
    fn abort_session(&mut self) {
        let targets = self.session.take().unwrap_or_default();
        if let Some(snapshot) = self.undo.pop() {
            self.ws = snapshot.ws;
            self.cursor = snapshot.cursor;
            self.cursor_hint = snapshot.cursor_hint;
        }
        // The insert may already have been written, so the rollback needs writing too.
        for target in targets {
            self.dirty.mark(target);
        }
    }

    fn resolve_cursor(&mut self, how: ResolveCursor, before: Option<usize>) {
        let ids = self.order_ids();
        if ids.is_empty() {
            self.cursor = None;
            self.cursor_hint = 0;
            return;
        }
        let last = ids.len() - 1;
        let by_index = before.unwrap_or(self.cursor_hint).min(last);
        let index = match how {
            ResolveCursor::ByIndex => by_index,
            ResolveCursor::ById => self
                .cursor
                .as_ref()
                .and_then(|id| ids.iter().position(|other| other == id))
                .unwrap_or(by_index),
        };
        self.cursor = ids.get(index).cloned();
        self.cursor_hint = index;
    }

    fn set_cursor_index(&mut self, index: usize) {
        let ids = self.order_ids();
        if ids.is_empty() {
            self.cursor = None;
            self.cursor_hint = 0;
            return;
        }
        let index = index.min(ids.len() - 1);
        self.cursor = ids.get(index).cloned();
        self.cursor_hint = index;
    }

    fn refresh_scroll(&mut self, now: i64) {
        let layout = self.layout(now);
        let viewport = self.geo.viewport_height();
        self.scroll = match self.cursor.as_ref().and_then(|id| layout.span(id)) {
            Some(span) => adjust_scroll(self.scroll, &span, layout.len(), viewport),
            None => clamp_scroll(self.scroll, layout.len(), viewport),
        };
    }

    fn new_toast(&mut self, text: impl Into<String>, level: ToastLevel) -> Toast {
        self.toast_seq = self.toast_seq.wrapping_add(1);
        let toast = Toast {
            text: text.into(),
            level,
            ttl_ms: match level {
                ToastLevel::Error => None,
                _ => Some(TOAST_TTL_MS),
            },
            seq: self.toast_seq,
        };
        self.toast = Some(toast.clone());
        toast
    }

    fn has_error_toast(&self) -> bool {
        self.toast
            .as_ref()
            .is_some_and(|t| t.level == ToastLevel::Error)
    }

    // --- views ---

    fn view_state(&self) -> ViewState {
        ViewState {
            cursor: self.cursor.clone(),
            cursor_hint: self.cursor_hint,
            scroll: self.scroll,
            search: self.filter().unwrap_or_default().to_string(),
        }
    }

    /// Switching views is a projection over what is already in memory. The Elixir
    /// version re-read `all-todos.json` on every `j`/`k` in the sidebar.
    fn switch_view(&mut self, view: ViewId) {
        if view == self.view {
            return;
        }
        let saved = self.view_state();
        match self.views.iter_mut().find(|(id, _)| id == &self.view) {
            Some((_, slot)) => *slot = saved,
            None => self.views.push((self.view.clone(), saved)),
        }

        let restored = self
            .views
            .iter()
            .find(|(id, _)| id == &view)
            .map(|(_, state)| state.clone())
            .unwrap_or_default();

        self.view = view;
        self.main = if restored.search.is_empty() {
            MainState::Normal
        } else {
            MainState::SearchNav {
                query: restored.search,
            }
        };
        self.cursor = restored.cursor;
        self.cursor_hint = restored.cursor_hint;
        self.scroll = restored.scroll;
        self.resolve_cursor(ResolveCursor::ById, None);
    }

    fn sidebar_items(&self) -> Vec<SidebarCursor> {
        let mut items = vec![SidebarCursor::All];
        items.extend(
            self.ws
                .projects()
                .flat_ordered()
                .iter()
                .map(|p| SidebarCursor::Project(p.id.clone())),
        );
        items
    }

    fn sidebar_index(&self) -> usize {
        self.sidebar_items()
            .iter()
            .position(|item| item == &self.sidebar_cursor)
            .unwrap_or(0)
    }

    fn selected_project(&self) -> Option<ProjectId> {
        match &self.sidebar_cursor {
            SidebarCursor::All => None,
            SidebarCursor::Project(id) => Some(id.clone()),
        }
    }

    fn view_for_sidebar(&self) -> ViewId {
        match &self.sidebar_cursor {
            SidebarCursor::All => ViewId::All,
            SidebarCursor::Project(id) => ViewId::Project(id.clone()),
        }
    }

    /// Where `a` puts a new todo: directly below the cursor, in the cursor's own
    /// section, so adding inside a project section stays inside it.
    fn insert_point(&self) -> (Bucket, usize) {
        let dl = self.display();
        let index = self
            .cursor_index()
            .unwrap_or(0)
            .min(dl.active.len().saturating_sub(1));
        if let Some(entry) = dl.active.get(index)
            && let Some((bucket, position)) = self.ws.find(&entry.todo.id)
        {
            return (bucket, position + 1);
        }
        let bucket = match &self.view {
            ViewId::All => Bucket::All,
            ViewId::Project(id) => Bucket::Project(id.clone()),
        };
        let len = self.ws.todos(&bucket).len();
        (bucket, len)
    }
}

impl Editing {
    #[must_use]
    pub fn id(&self) -> &TodoId {
        match &self.target {
            EditTarget::New(id) | EditTarget::Existing { id, .. } => id,
        }
    }
}

fn workspace_from(snapshot: &StoreSnapshot) -> Workspace {
    let projects =
        crate::project::Projects::new(snapshot.projects.iter().map(ProjectFile::project).collect());
    let todos = snapshot
        .projects
        .iter()
        .map(|file| (file.id.clone(), file.todos.clone()))
        .collect();
    Workspace::new(snapshot.all_todos.clone(), projects, todos)
}

fn toast_level(severity: crate::store::Severity) -> ToastLevel {
    match severity {
        crate::store::Severity::Warning => ToastLevel::Warning,
        crate::store::Severity::Error => ToastLevel::Error,
    }
}

/// Applies one action and reports the IO it implies.
pub fn reduce(state: &mut AppState, action: &Action, now: i64) -> Vec<Effect> {
    let before = state.cursor_index();

    // Any key other than a second `q` disarms the quit confirmation.
    if !matches!(action, Action::Quit) {
        state.quit_armed = false;
    }

    let effects = match chrome(state, action) {
        Some(effects) => effects,
        None => content(state, action, now),
    };

    state.resolve_cursor(action.resolve_cursor(), before);
    state.refresh_scroll(now);

    let (deletes, saves) = state.dirty.drain();
    let mut io: Vec<Effect> = deletes.into_iter().map(Effect::DeleteProject).collect();
    io.extend(
        saves
            .into_iter()
            .filter(|target| !state.ws.is_read_only(&bucket_of(target)))
            .map(Effect::Save),
    );
    io.extend(effects);
    io
}

/// Actions that are about the window rather than the list. `None` means the action
/// belongs to the panes.
fn chrome(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    let mut effects = Vec::new();
    match action {
        Action::Resize(width, height) => {
            state.geo.term_width = *width;
            state.geo.term_height = *height;
        }
        Action::ToastExpire(seq) => {
            if state.toast.as_ref().is_some_and(|t| t.seq == *seq) {
                state.toast = None;
            }
        }
        Action::DayChanged => {}

        Action::ToggleSidebar => {
            state.geo.sidebar_open = !state.geo.sidebar_open;
            state.pane = if state.sidebar_open() {
                Pane::Sidebar
            } else {
                state.sidebar = SidebarState::Normal;
                Pane::Main
            };
        }
        Action::SwitchFocus => {
            state.pane = match state.pane {
                Pane::Main => Pane::Sidebar,
                Pane::Sidebar => Pane::Main,
            };
            state.main = MainState::Normal;
        }
        Action::ToggleHelp => state.help = !state.help,

        Action::Quit => {
            if state.has_error_toast() && !state.quit_armed {
                state.quit_armed = true;
                effects.push(Effect::Toast(state.new_toast(
                    "-- save failed; press q again to quit anyway --",
                    ToastLevel::Warning,
                )));
            } else {
                effects.push(Effect::Quit);
            }
        }
        Action::ForceQuit => effects.push(Effect::Quit),
        _ => return None,
    }
    Some(effects)
}

fn content(state: &mut AppState, action: &Action, now: i64) -> Vec<Effect> {
    let mut effects = Vec::new();
    match action {
        Action::Cursor(motion) | Action::ExtendVisual(motion) => move_cursor(state, *motion),

        Action::AddTodo => add_todo(state, now),
        Action::EditTodo => edit_todo(state),
        Action::DeleteTodo | Action::DeleteSelected => delete(state),
        Action::ToggleTodo | Action::ToggleSelected => toggle(state, now),
        Action::Move(dir) => effects.extend(move_todos(state, *dir)),

        Action::EnterVisual => {
            if let Some(cursor) = state.cursor.clone() {
                state.main = MainState::Visual { anchor: cursor };
            }
        }
        Action::ExitVisual => state.main = MainState::Normal,

        Action::Edit(key) => {
            if let MainState::Insert(editing) = &mut state.main {
                apply_edit_key(&mut editing.input, *key);
            }
        }
        Action::ConfirmEdit => confirm_edit(state),
        Action::CancelEdit => cancel_edit(state),

        Action::EnterSearch => {
            let query = state.filter().unwrap_or_default().to_string();
            state.main = MainState::Search(TextInput::new(query));
        }
        Action::Search(key) => {
            if let MainState::Search(input) = &mut state.main {
                apply_edit_key(input, *key);
            }
            // The filtered list can shrink under the cursor, so re-anchor it every
            // keystroke rather than letting the highlight disappear.
            state.resolve_cursor(ResolveCursor::ById, Some(0));
        }
        Action::ConfirmSearch => {
            if let MainState::Search(input) = &state.main {
                state.main = MainState::SearchNav {
                    query: input.text().to_string(),
                };
                state.set_cursor_index(0);
            }
        }
        Action::CancelSearch => {
            state.main = MainState::Normal;
            state.resolve_cursor(ResolveCursor::ById, None);
        }

        Action::Undo => effects.extend(undo(state, true)),
        Action::Redo => effects.extend(undo(state, false)),

        Action::Sidebar(sidebar) => effects.extend(reduce_sidebar(state, *sidebar)),
        _ => {}
    }
    effects
}

fn project_ids(ws: &Workspace) -> Vec<ProjectId> {
    ws.projects()
        .as_slice()
        .iter()
        .map(|p| p.id.clone())
        .collect()
}

/// The file a todo's own bucket writes to.
fn touched_for(ws: &Workspace, id: &TodoId) -> Touched {
    match ws.find(id) {
        Some((bucket, _)) => Touched::saving(target_of(&bucket)),
        None => Touched::nothing(),
    }
}

fn touched_for_all(ws: &Workspace, ids: &[TodoId]) -> Touched {
    let mut touched = Touched::nothing();
    for id in ids {
        if let Some((bucket, _)) = ws.find(id) {
            touched = touched.and_save(target_of(&bucket));
        }
    }
    touched
}

/// Whether the project has a sibling at its own level to trade places with.
fn can_reorder_project(state: &AppState, id: &ProjectId, down: bool) -> bool {
    let Some(project) = state.ws.projects().get(id) else {
        return false;
    };
    let level: Vec<ProjectId> = match &project.parent_id {
        None => state.ws.projects().top_level(),
        Some(parent) => state.ws.projects().children(parent),
    }
    .iter()
    .map(|p| p.id.clone())
    .collect();

    let Some(at) = level.iter().position(|other| other == id) else {
        return false;
    };
    if down { at + 1 < level.len() } else { at > 0 }
}

fn bucket_of(target: &Target) -> Bucket {
    match target {
        Target::AllTodos => Bucket::All,
        Target::Project(id) => Bucket::Project(id.clone()),
    }
}

fn move_cursor(state: &mut AppState, motion: Motion) {
    let ids = state.order_ids();
    if ids.is_empty() {
        return;
    }
    let last = ids.len() - 1;
    let current = state.cursor_index().unwrap_or(0);
    // Half-page uses the viewport, not the terminal height; the Elixir version
    // overshot by the reserved rows.
    let jump = state.geo.viewport_height() / 2;
    let next = match motion {
        Motion::Down => current.saturating_add(1).min(last),
        Motion::Up => current.saturating_sub(1),
        Motion::Start => 0,
        Motion::End => last,
        Motion::HalfDown => current.saturating_add(jump).min(last),
        Motion::HalfUp => current.saturating_sub(jump),
    };
    state.set_cursor_index(next);
}

fn add_todo(state: &mut AppState, now: i64) {
    let (bucket, at) = state.insert_point();
    let todo = Todo::new("", now);
    let id = todo.id.clone();

    state.begin_session();
    let target = target_of(&bucket);
    state.mutate(|ws| {
        ws.insert_todo(&bucket, at, todo);
        ((), Touched::saving(target))
    });

    state.cursor = Some(id.clone());
    state.main = MainState::Insert(Editing {
        target: EditTarget::New(id),
        input: TextInput::new(""),
    });
}

fn edit_todo(state: &mut AppState) {
    let Some(id) = state.cursor.clone() else {
        return;
    };
    let Some(todo) = state.ws.get(&id) else {
        return;
    };
    let original = todo.text.clone();
    state.main = MainState::Insert(Editing {
        target: EditTarget::Existing {
            id,
            original: original.clone(),
        },
        input: TextInput::new(original),
    });
}

fn confirm_edit(state: &mut AppState) {
    let MainState::Insert(editing) = std::mem::take(&mut state.main) else {
        return;
    };
    let blank = editing.input.is_blank();
    let id = editing.id().clone();
    let text = editing.input.into_text();

    match editing.target {
        // Confirming an empty new todo discards it, exactly as escaping would.
        EditTarget::New(_) if blank => state.abort_session(),
        EditTarget::New(_) => {
            state.mutate(|ws| {
                ws.set_text(&id, text);
                ((), touched_for(ws, &id))
            });
            state.end_session();
        }
        // An emptied existing todo reverts rather than vanishing.
        EditTarget::Existing { original, .. } if blank || text == original => {}
        EditTarget::Existing { .. } => {
            state.mutate(|ws| {
                ws.set_text(&id, text);
                ((), touched_for(ws, &id))
            });
        }
    }
}

fn cancel_edit(state: &mut AppState) {
    let MainState::Insert(editing) = std::mem::take(&mut state.main) else {
        return;
    };
    if matches!(editing.target, EditTarget::New(_)) {
        state.abort_session();
    }
}

fn apply_edit_key(input: &mut TextInput, key: EditKey) {
    match key {
        EditKey::Char(ch) => input.insert_char(ch),
        EditKey::Backspace => input.backspace(),
        EditKey::Delete => input.delete(),
        EditKey::Left => input.move_left(),
        EditKey::Right => input.move_right(),
        EditKey::Home => input.move_home(),
        EditKey::End => input.move_end(),
        EditKey::DeleteWordBefore => input.delete_word_before(),
        EditKey::DeleteToStart => input.delete_to_start(),
    }
}

fn delete(state: &mut AppState) {
    let ids = state.selected_ids();
    if ids.is_empty() {
        return;
    }
    state.mutate(|ws| {
        // Read before the removal: afterwards the todos have no bucket to name.
        let touched = touched_for_all(ws, &ids);
        for id in &ids {
            ws.remove_todo(id);
        }
        ((), touched)
    });
    state.main = MainState::Normal;
}

fn toggle(state: &mut AppState, now: i64) {
    let ids = state.selected_ids();
    if ids.is_empty() {
        return;
    }
    state.mutate(|ws| {
        for id in &ids {
            ws.toggle(id, now);
        }
        ((), touched_for_all(ws, &ids))
    });
    state.main = MainState::Normal;
}

fn move_todos(state: &mut AppState, dir: Dir) -> Vec<Effect> {
    let Some(selection) = state.selection() else {
        return Vec::new();
    };
    let plan = {
        let dl = state.display();
        reorder::plan(&state.ws, &state.view, &dl, selection, dir)
    };

    match plan {
        Reorder::Blocked(Blocked::Completed) => vec![Effect::Toast(
            state.new_toast("-- completed todos can't be reordered --", ToastLevel::Info),
        )],
        Reorder::Blocked(Blocked::Edge) => Vec::new(),
        Reorder::Move { ref bucket, .. } => {
            let target = target_of(bucket);
            state.mutate(|ws| {
                reorder::apply(ws, &plan);
                ((), Touched::saving(target))
            });
            Vec::new()
        }
        Reorder::Reassign {
            ref to, ref ids, ..
        } => {
            let label = section_label(state, to);
            let destination = target_of(to);
            state.mutate(|ws| {
                let touched = touched_for_all(ws, ids).and_save(destination);
                reorder::apply(ws, &plan);
                ((), touched)
            });
            vec![Effect::Toast(state.new_toast(
                format!("-- moved to {label} --"),
                ToastLevel::Info,
            ))]
        }
    }
}

fn section_label(state: &AppState, bucket: &Bucket) -> String {
    match bucket.project() {
        None => "Todos".into(),
        Some(id) => state
            .ws
            .projects()
            .get(id)
            .map_or_else(|| "Todos".into(), |p| format!("# {}", p.name)),
    }
}

fn undo(state: &mut AppState, backwards: bool) -> Vec<Effect> {
    let current = state.snapshot();
    let restored = if backwards {
        state.undo.undo(current)
    } else {
        state.undo.redo(current)
    };
    let Some(snapshot) = restored else {
        let message = if backwards {
            "-- already at the oldest change --"
        } else {
            "-- already at the newest change --"
        };
        return vec![Effect::Toast(state.new_toast(message, ToastLevel::Info))];
    };
    state.restore(snapshot);
    state.resolve_cursor(ResolveCursor::ById, None);
    Vec::new()
}

fn reduce_sidebar(state: &mut AppState, action: SidebarAction) -> Vec<Effect> {
    match action {
        SidebarAction::Down | SidebarAction::Up => {
            let items = state.sidebar_items();
            let index = state.sidebar_index();
            let next = match action {
                SidebarAction::Down => index.saturating_add(1).min(items.len().saturating_sub(1)),
                _ => index.saturating_sub(1),
            };
            if let Some(item) = items.get(next) {
                state.sidebar_cursor = item.clone();
            }
            let view = state.view_for_sidebar();
            state.switch_view(view);
            Vec::new()
        }
        SidebarAction::Select => {
            state.pane = Pane::Main;
            Vec::new()
        }
        SidebarAction::Edit(key) => {
            if let SidebarState::Insert { input, .. } = &mut state.sidebar {
                apply_edit_key(input, key);
            }
            Vec::new()
        }
        SidebarAction::ConfirmEdit => confirm_sidebar_edit(state),
        SidebarAction::CancelDelete | SidebarAction::CancelEdit => {
            state.sidebar = SidebarState::Normal;
            Vec::new()
        }
        _ => sidebar_project_op(state, action),
    }
}

fn sidebar_project_op(state: &mut AppState, action: SidebarAction) -> Vec<Effect> {
    match action {
        SidebarAction::AddProject => {
            state.sidebar = SidebarState::Insert {
                target: ProjectEdit::NewTopLevel,
                input: TextInput::new(""),
            };
            Vec::new()
        }
        SidebarAction::AddSubproject => match state.selected_project() {
            Some(id)
                if state
                    .ws
                    .projects()
                    .get(&id)
                    .is_some_and(Project::is_top_level) =>
            {
                state.sidebar = SidebarState::Insert {
                    target: ProjectEdit::NewChild(id),
                    input: TextInput::new(""),
                };
                Vec::new()
            }
            Some(_) => vec![Effect::Toast(
                state.new_toast("-- only two levels of projects --", ToastLevel::Info),
            )],
            None => vec![Effect::Toast(state.new_toast(
                "-- \"All Todos\" can't hold subprojects --",
                ToastLevel::Info,
            ))],
        },
        SidebarAction::Rename => match state.selected_project() {
            Some(id) => {
                let name = state
                    .ws
                    .projects()
                    .get(&id)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                state.sidebar = SidebarState::Insert {
                    target: ProjectEdit::Rename(id),
                    input: TextInput::new(name),
                };
                Vec::new()
            }
            None => vec![Effect::Toast(
                state.new_toast("-- \"All Todos\" can't be renamed --", ToastLevel::Info),
            )],
        },
        SidebarAction::Delete => match state.selected_project() {
            Some(id) if state.ws.has_open_todos(&id) => {
                state.sidebar = SidebarState::ConfirmDelete(id);
                Vec::new()
            }
            Some(id) => delete_project(state, &id),
            None => vec![Effect::Toast(
                state.new_toast("-- \"All Todos\" can't be deleted --", ToastLevel::Info),
            )],
        },
        SidebarAction::ConfirmDelete => {
            let SidebarState::ConfirmDelete(id) = std::mem::take(&mut state.sidebar) else {
                return Vec::new();
            };
            delete_project(state, &id)
        }
        SidebarAction::Move(dir) => {
            let Some(id) = state.selected_project() else {
                return vec![Effect::Toast(state.new_toast(
                    "-- \"All Todos\" can't be reordered --",
                    ToastLevel::Info,
                ))];
            };
            let down = dir == Dir::Down;
            // Decided before mutating: entering `mutate` speculatively would clear the
            // redo branch even when the project is already at the end of its level.
            if !can_reorder_project(state, &id, down) {
                return Vec::new();
            }
            // Both projects in the swap change index, so both files need writing.
            let files: Vec<Target> = state
                .ws
                .projects()
                .as_slice()
                .iter()
                .map(|p| Target::Project(p.id.clone()))
                .collect();
            state.mutate(|ws| {
                let moved = ws.reorder_project(&id, down);
                debug_assert!(moved, "a reorder ruled possible did not happen");
                ((), Touched::saving_all(files))
            });
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn confirm_sidebar_edit(state: &mut AppState) -> Vec<Effect> {
    let SidebarState::Insert { target, input } = std::mem::take(&mut state.sidebar) else {
        return Vec::new();
    };
    let name = input.text().trim().to_string();
    if name.is_empty() {
        return Vec::new();
    }

    match target {
        ProjectEdit::NewTopLevel | ProjectEdit::NewChild(_) => {
            let parent = match &target {
                ProjectEdit::NewChild(id) => Some(id.clone()),
                _ => None,
            };
            let id = state.mutate(|ws| {
                let id = ws.add_project(name, parent);
                let touched = Touched::saving(Target::Project(id.clone()));
                (id, touched)
            });
            state.sidebar_cursor = SidebarCursor::Project(id);
            let view = state.view_for_sidebar();
            state.switch_view(view);
        }
        ProjectEdit::Rename(id) => {
            state.mutate(|ws| {
                ws.rename_project(&id, name);
                ((), Touched::saving(Target::Project(id.clone())))
            });
        }
    }
    Vec::new()
}

fn delete_project(state: &mut AppState, id: &ProjectId) -> Vec<Effect> {
    let items = state.sidebar_items();
    let index = state.sidebar_index();
    let removed = state.mutate(|ws| {
        let removed = ws.delete_project(id);
        (removed.clone(), Touched::deleting(removed))
    });

    // Land on whatever now occupies the deleted row, which is the entry below it.
    let survivors = state.sidebar_items();
    state.sidebar_cursor = survivors
        .get(index.min(survivors.len().saturating_sub(1)))
        .cloned()
        .unwrap_or(SidebarCursor::All);
    drop(items);

    state.views.retain(|(view, _)| match view {
        ViewId::All => true,
        ViewId::Project(project) => !removed.contains(project),
    });
    let view = state.view_for_sidebar();
    state.switch_view(view);
    if matches!(&state.view, ViewId::Project(project) if removed.contains(project)) {
        state.switch_view(ViewId::All);
    }

    state.sidebar = SidebarState::Normal;
    vec![Effect::Toast(
        state.new_toast("-- project deleted --", ToastLevel::Info),
    )]
}
