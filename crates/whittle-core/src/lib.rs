//! Whittle kernel.
//!
//! Refer to `docs/IDEA.md` and `docs/ARCHITECTURE.md` at the
//! repository root for the specification this crate implements.

#![no_std]

extern crate alloc;

pub mod composition;
#[macro_use]
mod macros;
pub mod primitive;
mod rule;
pub mod transform;

pub use composition::{And, Not, Or};
#[cfg(feature = "proptest")]
pub use rule::ArbitraryRule;
pub use rule::{Refined, Rule};
pub use transform::{StableUnderAsciiLowercase, StableUnderAsciiUppercase, StableUnderTrim};
