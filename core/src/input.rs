//! The keymap: one pure function from a keypress to an `Action`.
//!
//! Port of `event_mapping.ex` + `sidebar_event_mapping.ex`. Dispatch order matters and
//! mirrors the original: help swallows everything, then the global keys (which require
//! both panes idle), then the focused pane.

use crate::action::{Action, Dir, EditKey, Motion, SidebarAction};
use crate::mode::{Focus, MainMode, SidebarMode};

/// Terminal-agnostic key. The cli converts `crossterm::event::KeyEvent` into this at
/// the process boundary so `core` never sees a terminal type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPress {
    pub code: KeyCode,
    pub mods: Mods,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// Shift is deliberately absent: for a character key the case already carries it, and
/// terminals disagree about whether `J` also reports SHIFT. The cli drops the shift bit
/// when converting, so `J` is only ever `Char('J')` with no modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
    };

    #[must_use]
    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}

impl KeyPress {
    #[must_use]
    pub fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: Mods::NONE,
        }
    }

    #[must_use]
    pub fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: Mods::CTRL,
        }
    }

    #[must_use]
    pub fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }
}

/// The read-only slice of app state the keymap needs. Nothing about the todos, the
/// projects or the edit buffers reaches this function, which is what lets the whole
/// table be tested as data.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputContext {
    pub focus: Focus,
    pub sidebar_open: bool,
    pub help: bool,
}

#[must_use]
pub fn map(ctx: InputContext, key: KeyPress) -> Option<Action> {
    // ctrl+c is the one key that outranks every mode, including an open help overlay.
    if key.mods.ctrl && key.code == KeyCode::Char('c') {
        return Some(Action::ForceQuit);
    }

    if ctx.help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Escape if key.mods.is_none() => Some(Action::ToggleHelp),
            _ => None,
        };
    }

    if ctx.focus.is_idle()
        && key.mods.is_none()
        && let Some(action) = global(ctx, key.code)
    {
        return Some(action);
    }

    match ctx.focus {
        Focus::Sidebar(mode) => sidebar(mode, key),
        Focus::Main(mode) => main(ctx, mode, key),
    }
}

fn global(ctx: InputContext, code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('\\') => Some(Action::ToggleSidebar),
        KeyCode::Tab if ctx.sidebar_open => Some(Action::SwitchFocus),
        _ => None,
    }
}

fn main(ctx: InputContext, mode: MainMode, key: KeyPress) -> Option<Action> {
    match mode {
        MainMode::Normal => main_normal(ctx, key),
        MainMode::Insert => match key.code {
            KeyCode::Enter if key.mods.is_none() => Some(Action::ConfirmEdit),
            KeyCode::Escape if key.mods.is_none() => Some(Action::CancelEdit),
            _ => edit_key(key).map(Action::Edit),
        },
        MainMode::Visual => main_visual(key),
        MainMode::Search => match key.code {
            KeyCode::Enter if key.mods.is_none() => Some(Action::ConfirmSearch),
            KeyCode::Escape if key.mods.is_none() => Some(Action::CancelSearch),
            _ => edit_key(key).map(Action::Search),
        },
        MainMode::SearchNav => match (key.code, key.mods.is_none()) {
            (KeyCode::Char('j') | KeyCode::Down, true) => Some(Action::Cursor(Motion::Down)),
            (KeyCode::Char('k') | KeyCode::Up, true) => Some(Action::Cursor(Motion::Up)),
            (KeyCode::Char('/'), true) => Some(Action::EnterSearch),
            (KeyCode::Escape, true) => Some(Action::CancelSearch),
            _ => None,
        },
    }
}

fn main_normal(ctx: InputContext, key: KeyPress) -> Option<Action> {
    if key.mods.ctrl {
        return match key.code {
            KeyCode::Char('d') => Some(Action::Cursor(Motion::HalfDown)),
            KeyCode::Char('u') => Some(Action::Cursor(Motion::HalfUp)),
            KeyCode::Char('r') => Some(Action::Redo),
            _ => None,
        };
    }
    if !key.mods.is_none() {
        return None;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Cursor(Motion::Down)),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Cursor(Motion::Up)),
        // `gg` needs no pending state: a top jump is idempotent, so the second `g`
        // lands where the first one did. Both spellings work, neither is nagged about.
        KeyCode::Char('g') => Some(Action::Cursor(Motion::Start)),
        KeyCode::Char('G') => Some(Action::Cursor(Motion::End)),
        KeyCode::Char('a') => Some(Action::AddTodo),
        KeyCode::Char('e' | 'i') => Some(Action::EditTodo),
        KeyCode::Char('d') => Some(Action::DeleteTodo),
        KeyCode::Char(' ') => Some(Action::ToggleTodo),
        KeyCode::Char('J') => Some(Action::Move(Dir::Down)),
        KeyCode::Char('K') => Some(Action::Move(Dir::Up)),
        KeyCode::Char('v') => Some(Action::EnterVisual),
        KeyCode::Char('/') => Some(Action::EnterSearch),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('h') | KeyCode::Left if ctx.sidebar_open => Some(Action::SwitchFocus),
        _ => None,
    }
}

fn main_visual(key: KeyPress) -> Option<Action> {
    if key.mods.ctrl {
        return match key.code {
            KeyCode::Char('j') | KeyCode::Down => Some(Action::Move(Dir::Down)),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::Move(Dir::Up)),
            _ => None,
        };
    }
    if !key.mods.is_none() {
        return None;
    }

    match key.code {
        KeyCode::Char('J') => Some(Action::Move(Dir::Down)),
        KeyCode::Char('K') => Some(Action::Move(Dir::Up)),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::ExtendVisual(Motion::Down)),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::ExtendVisual(Motion::Up)),
        KeyCode::Char('d') => Some(Action::DeleteSelected),
        KeyCode::Char(' ') => Some(Action::ToggleSelected),
        KeyCode::Escape => Some(Action::ExitVisual),
        _ => None,
    }
}

fn sidebar(mode: SidebarMode, key: KeyPress) -> Option<Action> {
    // One undo stack for both panes, so `u` here undoes project edits.
    if mode == SidebarMode::Normal {
        if key == KeyPress::char('u') {
            return Some(Action::Undo);
        }
        if key == KeyPress::ctrl('r') {
            return Some(Action::Redo);
        }
    }

    let action = match mode {
        SidebarMode::Normal => sidebar_normal(key)?,
        SidebarMode::Insert => match key.code {
            KeyCode::Enter if key.mods.is_none() => SidebarAction::ConfirmEdit,
            KeyCode::Escape if key.mods.is_none() => SidebarAction::CancelEdit,
            _ => SidebarAction::Edit(edit_key(key)?),
        },
        SidebarMode::ConfirmDelete => {
            if !key.mods.is_none() {
                return None;
            }
            match key.code {
                KeyCode::Char('y') => SidebarAction::ConfirmDelete,
                KeyCode::Char('n') | KeyCode::Escape => SidebarAction::CancelDelete,
                _ => return None,
            }
        }
    };
    Some(Action::Sidebar(action))
}

fn sidebar_normal(key: KeyPress) -> Option<SidebarAction> {
    if !key.mods.is_none() {
        return None;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(SidebarAction::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(SidebarAction::Up),
        KeyCode::Char('a') => Some(SidebarAction::AddProject),
        KeyCode::Char('s') => Some(SidebarAction::AddSubproject),
        KeyCode::Char('e' | 'i') => Some(SidebarAction::Rename),
        KeyCode::Char('d') => Some(SidebarAction::Delete),
        KeyCode::Char('J') => Some(SidebarAction::Move(Dir::Down)),
        KeyCode::Char('K') => Some(SidebarAction::Move(Dir::Up)),
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Some(SidebarAction::Select),
        _ => None,
    }
}

/// The editing primitives every text field shares.
fn edit_key(key: KeyPress) -> Option<EditKey> {
    if key.mods.ctrl {
        return match key.code {
            KeyCode::Char('w') => Some(EditKey::DeleteWordBefore),
            KeyCode::Char('u') => Some(EditKey::DeleteToStart),
            KeyCode::Char('a') => Some(EditKey::Home),
            KeyCode::Char('e') => Some(EditKey::End),
            _ => None,
        };
    }
    if !key.mods.is_none() {
        return None;
    }

    match key.code {
        // Any char, including composed and multi-byte: the Elixir `byte_size(key) == 1`
        // filter silently dropped every non-ASCII keystroke.
        KeyCode::Char(c) => Some(EditKey::Char(c)),
        KeyCode::Backspace => Some(EditKey::Backspace),
        KeyCode::Delete => Some(EditKey::Delete),
        KeyCode::Left => Some(EditKey::Left),
        KeyCode::Right => Some(EditKey::Right),
        KeyCode::Home => Some(EditKey::Home),
        KeyCode::End => Some(EditKey::End),
        _ => None,
    }
}
