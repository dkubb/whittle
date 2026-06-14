//! Library-supplied primitive rules.

pub mod collection;
#[cfg(feature = "chrono")]
pub mod date;
#[cfg(feature = "chrono")]
pub mod datetime;
#[cfg(feature = "decimal")]
pub mod decimal;
pub mod float;
pub mod numeric;
pub mod path;
#[cfg(feature = "regex")]
pub mod pattern;
pub mod string;
#[cfg(feature = "url")]
pub mod url;

pub use collection::{
    AllItems, AnyOf, CapacityFull, CollectionError, Distinct, IdentityKey, KeyOf, LenItems, NoneOf,
    Predicate, Sorted, StableUnderElementMap, UniqueByKey,
};
#[cfg(all(feature = "chrono", feature = "proptest"))]
pub use date::ArbitraryDate;
#[cfg(feature = "chrono")]
pub use date::{DateAtLeast, DateAtMost, DateError, DateInRange};
#[cfg(all(feature = "chrono", feature = "proptest"))]
pub use datetime::ArbitraryDateTime;
#[cfg(feature = "chrono")]
pub use datetime::{DateTimeAtLeast, DateTimeAtMost, DateTimeError, DateTimeInRange};
#[cfg(all(feature = "decimal", feature = "proptest"))]
pub use decimal::ArbitraryDecimal;
#[cfg(feature = "decimal")]
pub use decimal::{DecimalError, DecimalInRange, DecimalPositive, DecimalPrecision, DecimalScale};
#[cfg(feature = "proptest")]
pub use float::ArbitraryFloat;
pub use float::{Finite, Float, FloatError, InClosedRange, NotInfinite, NotNan};
#[cfg(feature = "proptest")]
pub use numeric::ArbitraryNumeric;
pub use numeric::{
    AtLeast, AtMost, EqualTo, GreaterThan, LessThan, Negative, NonZero, NotEqualTo, Numeric,
    NumericError, Positive, Within,
};
pub use path::{PathError, RelativePath};
#[cfg(feature = "regex")]
pub use pattern::{Pattern, PatternError};
#[cfg(feature = "proptest")]
pub use string::ArbitraryChar;
pub use string::{
    AsciiAlphabetic, AsciiAlphanumeric, AsciiDigit, AsciiGraphic, AsciiLowercase, AsciiUppercase,
    CharEither, CharExcept, CharLiteral, CharPredicate, EachChar, FirstChar, IdentChar,
    IdentDashChar, IdentStart, LenBytes, LenChars, NonControl, NonEmpty, RejectsTrimWhitespace,
    SchemaChar, StringError,
};
#[cfg(feature = "unicode")]
pub use string::{BoundedLine, BoundedText, PrintableChar, PrintableLine, PrintableMultiline};
#[cfg(feature = "hex")]
pub use string::{HexChar, HexFixedAny, HexFixedLower, HexFixedNormalized};
#[cfg(feature = "url")]
pub use url::{HTTP_URL_MAX_LEN, HttpUrl, HttpUrlError};
