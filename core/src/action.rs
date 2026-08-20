//! The vocabulary the whole app reduces over.
//!
//! `input::map` produces these and nothing else; `app::reduce` consumes these and
//! nothing else. Anything a key can ask for has a name here, so "which key does what"
//! and "what does that do" stay separately testable.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Resize(u16, u16),
    /// A toast's lifetime ran out. Carries the toast's sequence number so a wakeup
    /// scheduled for a superseded toast can be dropped instead of clearing the new one.
    ToastExpire(u64),
    /// Midnight passed, so every "Nd" age label is stale. Redraw only.
    DayChanged,

    ToggleSidebar,
    SwitchFocus,
    ToggleHelp,
    Quit,
    ForceQuit,

    Cursor(Motion),
    AddTodo,
    EditTodo,
    DeleteTodo,
    ToggleTodo,
    Move(Dir),

    EnterVisual,
    ExitVisual,
    ExtendVisual(Motion),
    DeleteSelected,
    ToggleSelected,

    Edit(EditKey),
    ConfirmEdit,
    CancelEdit,

    EnterSearch,
    Search(EditKey),
    ConfirmSearch,
    CancelSearch,

    Undo,
    Redo,

    Sidebar(SidebarAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Down,
    Up,
    Start,
    End,
    HalfDown,
    HalfUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
}

/// One text-editing primitive. Shared by the todo editor, the project-name editor and
/// the search field so all three behave identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// `ctrl+w`
    DeleteWordBefore,
    /// `ctrl+u`
    DeleteToStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarAction {
    Down,
    Up,
    Select,
    AddProject,
    AddSubproject,
    Rename,
    Delete,
    ConfirmDelete,
    CancelDelete,
    Move(Dir),
    Edit(EditKey),
    ConfirmEdit,
    CancelEdit,
}

/// How the reducer re-establishes the cursor after an action has changed the list.
///
/// Identity is the default because the eye follows a todo that moved. The two
/// exceptions are the triage keys: `space` and `d` leave the cursor where it was on
/// screen so a run of them walks down the list, which is the behaviour the Elixir
/// version had by accident and the one worth keeping on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveCursor {
    ById,
    ByIndex,
}

impl Action {
    #[must_use]
    pub fn resolve_cursor(&self) -> ResolveCursor {
        match self {
            Self::ToggleTodo | Self::DeleteTodo | Self::ToggleSelected | Self::DeleteSelected => {
                ResolveCursor::ByIndex
            }
            _ => ResolveCursor::ById,
        }
    }
}
