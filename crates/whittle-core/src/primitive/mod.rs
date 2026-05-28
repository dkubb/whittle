//! Library-supplied primitive rules.

pub mod collection;
pub mod float;
pub mod numeric;
pub mod path;
pub mod string;

pub use collection::{
    AllItems, AnyOf, CollectionError, Distinct, IdentityKey, KeyOf, LenItems, NoneOf, Predicate,
    Sorted, UniqueByKey,
};
#[cfg(feature = "proptest")]
pub use float::FloatArbitrary;
pub use float::{Finite, Float, FloatError, InClosedRange, NotInfinite, NotNan};
#[cfg(feature = "proptest")]
pub use numeric::NumericArbitrary;
pub use numeric::{AtLeast, AtMost, Negative, NonZero, Numeric, NumericError, Positive, Within};
pub use path::{PathError, RelativePath};
#[cfg(feature = "proptest")]
pub use string::CharStrategy;
pub use string::{
    AsciiAlphanumeric, CharPredicate, EachChar, FirstChar, IdentChar, IdentDashChar, IdentStart,
    LenBytes, LenChars, NonControl, NonEmpty, StringError,
};
#[cfg(feature = "hex")]
pub use string::{HexChar, HexFixedAny, HexFixedLower, HexFixedNormalized};
#[cfg(feature = "unicode")]
pub use string::{PrintableLine, PrintableMultiline};
