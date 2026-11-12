//! A cycle-exact MOS 6510 / 6502 CPU core.

#![forbid(unsafe_code)]

#[allow(dead_code)] // used from Task 2, which replaces this file
mod ops;

pub use ops::UNSTABLE_MAGIC;
