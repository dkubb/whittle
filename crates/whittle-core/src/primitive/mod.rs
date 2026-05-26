//! Library-supplied primitive rules.

pub mod numeric;

pub use numeric::{
    AtLeast, AtMost, Negative, NonZero, Numeric, NumericError,
    Positive, Within,
};
