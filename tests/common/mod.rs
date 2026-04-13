// Each `tests/*.rs` integration test is its own crate and only uses a subset
// of these helpers; the rest get flagged as dead by that crate's lint pass.
#![allow(dead_code)]

pub mod harness;
