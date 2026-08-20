#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

//! Pure domain, layout and input logic for doer.
//!
//! This crate must never depend on a terminal: no `ratatui`, no `crossterm`, no
//! `std::fs`. Everything here is unit-testable without a screen, which is what keeps
//! the rendering layer honest.

pub mod action;
pub mod app;
pub mod dirty;
pub mod display;
pub mod help;
pub mod id;
pub mod input;
// An index into the row list or a bucket is a real bug if it is out of range, so these
// two modules ask for `.get()` rather than `[]`. Elsewhere an index is provably in range
// and `.get()` would be noise.
#[deny(clippy::indexing_slicing)]
pub mod layout;
pub mod mode;
pub mod project;
pub mod reorder;
pub mod store;
pub mod text;
pub mod todo;
pub mod undo;
#[deny(clippy::indexing_slicing)]
pub mod workspace;

pub use action::{Action, Dir, EditKey, Motion, ResolveCursor, SidebarAction};
pub use app::{AppState, Effect, Toast, ToastLevel, reduce};
pub use display::{DisplayList, ViewId};
pub use help::Section;
pub use id::{ProjectId, TodoId};
pub use input::{InputContext, KeyCode, KeyPress, Mods};
pub use mode::{Focus, MainMode, SidebarMode};
pub use project::{Project, Projects};
pub use store::{Loaded, Problem, ProjectFile, Severity, Store, StoreError, StoreSnapshot, Target};
pub use todo::Todo;
pub use workspace::{Bucket, Workspace};
