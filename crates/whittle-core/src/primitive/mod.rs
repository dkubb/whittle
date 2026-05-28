//! Library-supplied primitive rules.

pub mod collection;
pub mod float;
pub mod numeric;
pub mod string;

pub use collection::{
    AllItems, AnyOf, CollectionError, Distinct, IdentityKey, KeyOf, LenItems, NoneOf, Predicate,
    Sorted, UniqueByKey,
};
pub use float::{Finite, Float, FloatError, InClosedRange, NotInfinite, NotNan};
pub use numeric::{
    AtLeast, AtMost, Negative, NonZero, Numeric, NumericError,
    Positive, Within,
};
pub use string::{
    AsciiAlphanumeric, CharPredicate, EachChar, FirstChar, IdentChar,
    IdentStart, LenBytes, LenChars, NonControl, NonEmpty, StringError,
};
