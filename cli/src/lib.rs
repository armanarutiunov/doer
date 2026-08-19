#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Library face of the binary, so `tests/` can drive the store without a terminal.

pub mod store;
