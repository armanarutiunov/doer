#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// Removed once the event loop wires these together.
#![allow(dead_code)]

//! Library face of the binary, so `tests/` can drive the store and the renderer
//! without a terminal.

pub mod event;
pub mod store;
pub mod term;
pub mod ui;
