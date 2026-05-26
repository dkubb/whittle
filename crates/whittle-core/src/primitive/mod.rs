//! Library-supplied primitive rules.

pub mod collection;
pub mod numeric;
pub mod string;

pub use collection::{
    AllItems, CollectionError, KeyOf, LenItems, UniqueByKey,
};
pub use numeric::{
    AtLeast, AtMost, Negative, NonZero, Numeric, NumericError,
    Positive, Within,
};
pub use string::{
    AsciiAlphanumeric, CharPredicate, EachChar, IdentChar, LenBytes,
    LenChars, NonControl, NonEmpty, StringError,
};
