#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Pure domain, layout and input logic for doer.
//!
//! This crate must never depend on a terminal: no `ratatui`, no `crossterm`, no
//! `std::fs`. Everything here is unit-testable without a screen, which is what keeps
//! the rendering layer honest.

pub mod display;
pub mod id;
pub mod layout;
pub mod project;
pub mod text;
pub mod todo;
pub mod workspace;

pub use id::{ProjectId, TodoId};
pub use project::{Project, Projects};
pub use todo::Todo;
pub use workspace::{Bucket, Workspace};
