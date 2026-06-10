//! Whittle kernel.
//!
//! Refer to `docs/IDEA.md` and `docs/ARCHITECTURE.md` at the
//! repository root for the specification this crate implements.

#![no_std]
#![cfg_attr(feature = "regex", feature(adt_const_params, unsized_const_params))]
#![cfg_attr(
    feature = "regex",
    expect(
        incomplete_features,
        reason = "&'static str const generics carry the regex pattern in the type"
    )
)]

extern crate alloc;
// The `regex` rule needs the regex crate and a keyed `OnceLock` cache,
// both of which require `std`. The kernel stays `#![no_std]` by default
// and pulls `std` in ONLY when the `regex` feature is enabled.
#[cfg(feature = "regex")]
extern crate std;

pub mod composition;
#[macro_use]
mod macros;
pub mod primitive;
mod rule;
pub mod transform;

pub use composition::{All, And, Any, ErrorMapper, MapErr, Not, Or, Xor};
pub use primitive::StableUnderElementMap;
#[cfg(feature = "proptest")]
pub use rule::ArbitraryRule;
pub use rule::{Refined, Rule};
pub use transform::{StableUnderAsciiLowercase, StableUnderAsciiUppercase, StableUnderTrim};
/// Compile-time-validated constructor for [`primitive::Pattern`].
///
/// `pattern!(r"...")` expands to a `Pattern<RE>` rule type and rejects
/// a malformed regex as a compile error. See the macro's own docs in
/// `whittle-macros`, and the worked admit/reject + compile-fail
/// examples on the `whittle` facade's re-export (the macro resolves the
/// facade crate path, so its doctests run against `whittle::pattern!`).
#[cfg(feature = "regex")]
pub use whittle_macros::pattern;
