#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Pure domain, layout and input logic for doer.
//!
//! This crate must never depend on a terminal: no `ratatui`, no `crossterm`, no
//! `std::fs`. Everything here is unit-testable without a screen, which is what keeps
//! the rendering layer honest.
