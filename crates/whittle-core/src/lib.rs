//! Whittle kernel.
//!
//! Refer to `docs/IDEA.md` and `docs/ARCHITECTURE.md` at the
//! repository root for the specification this crate implements.

#![no_std]

extern crate alloc;

pub mod primitive;
mod rule;

pub use rule::{Refined, Rule};
