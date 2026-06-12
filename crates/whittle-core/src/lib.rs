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

pub mod closed_set;
pub mod composition;
pub mod implies;
#[macro_use]
mod macros;
pub mod primitive;
mod rule;
pub mod schema;
#[cfg(feature = "proptest")]
pub mod testing;
pub mod transform;

pub use closed_set::{ClosedSet, ClosedSetError};
pub use composition::{All, And, Any, ErrorMapper, MapErr, Not, Or, Xor};
pub use implies::Implies;
pub use primitive::StableUnderElementMap;
#[cfg(feature = "proptest")]
pub use rule::ArbitraryRule;
#[cfg(feature = "serde")]
pub use rule::{DeserializeRule, parse_then_refine};
pub use rule::{Refined, Rule};
pub use schema::{Schema, SchemaRule};
/// Re-export of `serde` so [`deserialize_rule!`] expansions resolve
/// serde paths through `$crate` without requiring downstream crates
/// to depend on `serde` directly.
#[cfg(feature = "serde")]
#[doc(hidden)]
pub use serde;
#[cfg(feature = "proptest")]
pub use testing::{
    assert_closed_set_schema, assert_schema_boundary_matrix, assert_schema_char,
    prop_image_refines, prop_schema_cross_check, prop_total,
};
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
