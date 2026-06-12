//! Numeric primitive rules.
//!
//! Bounded ranges (`Within<MIN, MAX>`, `AtLeast<MIN>`, `AtMost<MAX>`)
//! and sign / non-zero markers (`NonZero`, `Positive`, `Negative`).
//! Each primitive carries a typed error variant that includes the
//! offending value so callers can construct precise diagnostics.

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::{Refined, Rule};
use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

/// Inclusive numeric range: `MIN <= value <= MAX`.
///
/// `Within` is a nominal domain newtype. Internally it composes
/// `AtLeast<MIN>` and `AtMost<MAX>` via `And<...>`. Both inner rules
/// share `NumericError`, so the composition's error is `NumericError`
/// directly — the `And`/`Or` machinery is an implementation detail,
/// not part of the domain surface.
///
/// `MIN > MAX` fails to compile: the `refine` impl carries a
/// `const { assert!(MIN <= MAX) }` block that fires at
/// monomorphisation. Degenerate empty-range instantiations are
/// unrepresentable, so their error variant is too.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NumericError, Within};
///
/// // Admit: value is within the inclusive range.
/// let ok: Refined<i32, Within<0, 100>> = Refined::try_new(50).unwrap();
/// assert_eq!(*ok.as_inner(), 50);
///
/// // Reject above MAX.
/// let err = Refined::<i32, Within<0, 100>>::try_new(101).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 101 });
///
/// // Reject below MIN.
/// let err = Refined::<i32, Within<0, 100>>::try_new(-1).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: -1 });
/// ```
pub struct Within<const MIN: i128, const MAX: i128>;

/// Lower-bound rule: `MIN <= value`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{AtLeast, NumericError};
///
/// let ok: Refined<i32, AtLeast<10>> = Refined::try_new(10).unwrap();
/// assert_eq!(*ok.as_inner(), 10);
///
/// let err = Refined::<i32, AtLeast<10>>::try_new(9).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 9 });
/// ```
pub struct AtLeast<const MIN: i128>;

/// Upper-bound rule: `value <= MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{AtMost, NumericError};
///
/// let ok: Refined<i32, AtMost<100>> = Refined::try_new(100).unwrap();
/// assert_eq!(*ok.as_inner(), 100);
///
/// let err = Refined::<i32, AtMost<100>>::try_new(101).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 101 });
/// ```
pub struct AtMost<const MAX: i128>;

/// Open lower-bound rule: `MIN < value`.
///
/// The strict counterpart of [`AtLeast`]. Use when the bound itself
/// is inadmissible (e.g. "strictly positive", "strictly greater than
/// zero", "must exceed the previous timestamp").
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{GreaterThan, NumericError};
///
/// let ok: Refined<i32, GreaterThan<10>> = Refined::try_new(11).unwrap();
/// assert_eq!(*ok.as_inner(), 11);
///
/// // The bound itself is rejected.
/// let err = Refined::<i32, GreaterThan<10>>::try_new(10).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 10 });
/// ```
pub struct GreaterThan<const MIN: i128>;

/// Open upper-bound rule: `value < MAX`.
///
/// The strict counterpart of [`AtMost`]. Use when the bound itself
/// is inadmissible (e.g. "less than the array length", "less than
/// the page limit").
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{LessThan, NumericError};
///
/// let ok: Refined<i32, LessThan<100>> = Refined::try_new(99).unwrap();
/// assert_eq!(*ok.as_inner(), 99);
///
/// // The bound itself is rejected.
/// let err = Refined::<i32, LessThan<100>>::try_new(100).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 100 });
/// ```
pub struct LessThan<const MAX: i128>;

/// Singleton rule: admits only `value == N`.
///
/// Useful for marker fields (a fixed protocol version, a known
/// status code, a sentinel constant). `N` must fit in the carrier
/// type for the rule to admit any value at all; `EqualTo<300>` over
/// `u8` admits nothing because 300 exceeds `u8::MAX`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{EqualTo, NumericError};
///
/// let ok: Refined<u8, EqualTo<42>> = Refined::try_new(42).unwrap();
/// assert_eq!(*ok.as_inner(), 42);
///
/// // Any other value is rejected.
/// let err = Refined::<u8, EqualTo<42>>::try_new(7).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 7 });
/// ```
pub struct EqualTo<const N: i128>;

/// Exclusion rule: admits every value except `N`. The dual of
/// [`EqualTo`], defined as `Not<EqualTo<N>>`.
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NotEqualTo, NumericError};
///
/// let ok: Refined<i32, NotEqualTo<-1>> = Refined::try_new(0).unwrap();
/// assert_eq!(*ok.as_inner(), 0);
///
/// let err = Refined::<i32, NotEqualTo<-1>>::try_new(-1).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: -1 });
/// ```
pub type NotEqualTo<const N: i128> = crate::composition::Not<EqualTo<N>>;

/// Rejects zero — type alias for [`NotEqualTo<0>`].
///
/// `NonZero` is the conventional spelling of the exclude-zero rule.
/// The underlying machinery is [`NotEqualTo<0>`].
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NonZero, NumericError};
///
/// let ok: Refined<i32, NonZero> = Refined::try_new(-3).unwrap();
/// assert_eq!(*ok.as_inner(), -3);
///
/// let err = Refined::<i32, NonZero>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub type NonZero = NotEqualTo<0>;

/// `value > 0` — alias for [`GreaterThan<0>`].
///
/// `Positive` is the conventional spelling of the strict-positivity
/// rule. The underlying machinery is [`GreaterThan<0>`].
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{NumericError, Positive};
///
/// let ok: Refined<i32, Positive> = Refined::try_new(1).unwrap();
/// assert_eq!(*ok.as_inner(), 1);
///
/// // Zero is not positive.
/// let err = Refined::<i32, Positive>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub type Positive = GreaterThan<0>;

/// `value < 0` — alias for [`LessThan<0>`].
///
/// `Negative` is the conventional spelling of the strict-negativity
/// rule. The underlying machinery is [`LessThan<0>`].
///
/// # Examples
///
/// ```
/// use whittle_core::Refined;
/// use whittle_core::primitive::{Negative, NumericError};
///
/// let ok: Refined<i32, Negative> = Refined::try_new(-1).unwrap();
/// assert_eq!(*ok.as_inner(), -1);
///
/// // Zero is not negative.
/// let err = Refined::<i32, Negative>::try_new(0).unwrap_err();
/// assert_eq!(err, NumericError::OutOfRange { value: 0 });
/// ```
pub type Negative = LessThan<0>;

/// Error variants common to every numeric primitive.
///
/// The variant carries the offending value as `i128` because every
/// supported numeric type widens losslessly into `i128`. Invalid
/// rule configurations (e.g. `Within<MIN, MAX>` with `MIN > MAX`)
/// are rejected at compile time via `const { assert!(...) }`
/// blocks inside the affected `Rule::refine` impls, so their
/// error variant is unrepresentable.
#[derive(Debug, PartialEq, Eq)]
pub enum NumericError {
    /// Value lies outside the rule's admissible range.
    OutOfRange {
        /// Offending value widened losslessly into `i128`.
        value: i128,
    },
}

impl core::fmt::Display for NumericError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::OutOfRange { value } => {
                write!(f, "value {value} not in admissible range")
            }
        }
    }
}

impl core::error::Error for NumericError {}

/// Conversion from a concrete numeric type into and back out of
/// `i128`, used by every numeric primitive's `Rule` impl.
///
/// Implementations exist for the standard signed and unsigned
/// integer types. `u128` is deliberately omitted: it cannot
/// round-trip through `i128`. A future `WithinUnsigned` family will
/// cover the upper half of `u128`.
///
/// `into_i128` is infallible because every supported type widens
/// losslessly into `i128`. `usize` / `isize` would only fail on a
/// platform whose pointer width exceeds 128 bits, which does not
/// exist; the impl panics in that case rather than carrying a
/// permanently-dead error path through every `Rule::refine` site.
pub trait Numeric: Sized + 'static {
    /// Widen `self` into an `i128`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::Numeric;
    ///
    /// assert_eq!(<i32 as Numeric>::into_i128(42_i32), 42_i128);
    /// assert_eq!(<u64 as Numeric>::into_i128(u64::MAX), i128::from(u64::MAX));
    /// ```
    fn into_i128(self) -> i128;

    /// Narrow `value` back into `Self`, or return `OutOfRange` when
    /// `value` does not fit.
    ///
    /// # Errors
    ///
    /// Returns `NumericError::OutOfRange { value }` when `value`
    /// cannot be represented as `Self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use whittle_core::primitive::{Numeric, NumericError};
    ///
    /// // Admit: value fits in `i32`.
    /// assert_eq!(<i32 as Numeric>::from_i128(42), Ok(42));
    ///
    /// // Reject: value exceeds `i8::MAX`.
    /// assert_eq!(
    ///     <i8 as Numeric>::from_i128(200),
    ///     Err(NumericError::OutOfRange { value: 200 }),
    /// );
    /// ```
    fn from_i128(value: i128) -> Result<Self, NumericError>;
}

macro_rules! impl_numeric_signed {
    ($($ty:ty),+) => { $(
        impl Numeric for $ty {
            #[inline]
            fn into_i128(self) -> i128 {
                i128::from(self)
            }
            #[inline]
            fn from_i128(value: i128) -> Result<Self, NumericError> {
                <$ty>::try_from(value)
                    .map_err(|_| NumericError::OutOfRange { value })
            }
        }
    )+ };
}

macro_rules! impl_numeric_unsigned {
    ($($ty:ty),+) => { $(
        impl Numeric for $ty {
            #[inline]
            fn into_i128(self) -> i128 {
                i128::from(self)
            }
            #[inline]
            fn from_i128(value: i128) -> Result<Self, NumericError> {
                <$ty>::try_from(value)
                    .map_err(|_| NumericError::OutOfRange { value })
            }
        }
    )+ };
}

impl_numeric_signed!(i8, i16, i32, i64);
impl_numeric_unsigned!(u8, u16, u32, u64);

// i128 needs its own impl because i128::from(self) doesn't exist
// (it would be identity); ditto round-trip.
impl Numeric for i128 {
    #[inline]
    fn into_i128(self) -> i128 {
        self
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Ok(value)
    }
}

// usize / isize widen through their architecture-specific size.
// A const-asserted upper bound on `<int>::BITS` keeps the cast
// in `into_i128` provably lossless, so we can use `i128::from`
// on the corresponding fixed-width primitive without any
// fallible-conversion path.
impl Numeric for usize {
    #[inline]
    fn into_i128(self) -> i128 {
        const {
            assert!(Self::BITS <= 64, "usize wider than 64 bits is unsupported");
        };
        i128::from(self as u64)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

impl Numeric for isize {
    #[inline]
    fn into_i128(self) -> i128 {
        const {
            assert!(Self::BITS <= 64, "isize wider than 64 bits is unsupported");
        };
        i128::from(self as i64)
    }
    #[inline]
    fn from_i128(value: i128) -> Result<Self, NumericError> {
        Self::try_from(value).map_err(|_| NumericError::OutOfRange { value })
    }
}

// ─── Proptest support: `ArbitraryNumeric`. ───────────────────────
//
// Each numeric `Rule` admits a contiguous (or near-contiguous)
// integer range. `ArbitraryNumeric` exposes a per-type strategy that
// emits values within a clamped range, so rules can compose their
// admissible region from `i128` bounds without rejection sampling.

/// Numeric types that expose a `proptest` range strategy.
///
/// Implementations clamp the requested `[min, max]` bounds (widened
/// to `i128`) to the type's own range and return a strategy whose
/// values are guaranteed to fit in `Self`. Rules like `Within<MIN,
/// MAX>` and `Positive` use this trait to construct their
/// `ArbitraryRule` strategies without rejection sampling.
///
/// The returned strategy is **edge-biased**: the clamped endpoints
/// are each emitted with weight 1 against weight 8 for the full
/// inclusive range (the float closed-range precedent in
/// `primitive/float.rs`). Uniform sampling almost never lands
/// exactly on the bound of a wide range, so the off-by-one rule
/// bugs that live at MIN/MAX would otherwise go unexercised. Every
/// emitted value remains admissible — the bias only reweights the
/// admissible region. All the bounded numeric rules (`Within`,
/// `AtLeast`, `AtMost`, `GreaterThan`, `LessThan`) inherit the bias
/// through this trait.
///
/// `Copy` is required so the strategy can yield values through a
/// `fn(&T) -> bool` filter without `Clone`-ing.
///
/// Available behind the `proptest` feature.
#[cfg(feature = "proptest")]
pub trait ArbitraryNumeric: Numeric + Copy {
    /// Strategy type emitted by [`Self::arbitrary_in_range`].
    ///
    /// Pinned to `BoxedStrategy<Self>` so consumers see a stable
    /// type — `Within<MIN, MAX>: ArbitraryRule<T>` is then a
    /// `BoxedStrategy<T>` regardless of the concrete `T`, without
    /// associated-type-equality bounds at every use site.
    type RangeStrategy: proptest::strategy::Strategy<Value = Self>;

    /// Strategy that emits values in the inclusive `[min, max]`
    /// range, clamped to `Self`'s own range.
    ///
    /// If the clamped range is empty, implementations must still
    /// return a non-empty strategy (typically a single endpoint).
    /// Callers should not pass ranges that, after clamping, would
    /// be empty for the type they are generating; the rules in this
    /// module never do.
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy;
}

/// Edge-biased range strategy shared by every `ArbitraryNumeric`
/// impl: each clamped endpoint at weight 1, the full inclusive
/// range at weight 8 (R-T3; mirrors the float closed-range
/// precedent). Degenerate `lo == hi` windows are fine — all three
/// arms emit the same value.
#[cfg(feature = "proptest")]
macro_rules! edge_biased_range {
    ($lo:expr, $hi:expr) => {{
        proptest::prop_oneof![
            1 => proptest::strategy::Just($lo),
            1 => proptest::strategy::Just($hi),
            8 => $lo..=$hi,
        ]
        .boxed()
    }};
}

/// Generate `ArbitraryNumeric` impls for the supported integer
/// types. Each impl clamps the requested `[min, max]` bounds to the
/// type's own representable range before constructing an
/// edge-biased proptest range strategy.
#[cfg(feature = "proptest")]
macro_rules! impl_numeric_arbitrary {
    ($($ty:ty),+) => { $(
        impl ArbitraryNumeric for $ty {
            type RangeStrategy = proptest::strategy::BoxedStrategy<$ty>;

            #[inline]
            fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
                use proptest::strategy::Strategy as _;
                let ty_min = i128::from(<$ty>::MIN);
                let ty_max = i128::from(<$ty>::MAX);
                let lo = if min < ty_min { ty_min } else { min };
                let hi = if max > ty_max { ty_max } else { max };
                // `lo, hi` are clamped to `[ty_min, ty_max]`,
                // which fits `Self` by construction; the
                // `try_from` fallbacks pin to the type's endpoints
                // for safety even though they cannot be reached
                // here.
                let lo = <$ty>::try_from(lo).unwrap_or(<$ty>::MIN);
                let hi = <$ty>::try_from(hi).unwrap_or(<$ty>::MAX);
                edge_biased_range!(lo, hi)
            }
        }
    )+ };
}

#[cfg(feature = "proptest")]
impl_numeric_arbitrary!(i8, i16, i32, i64, u8, u16, u32, u64);

// `i128` cannot widen losslessly *from* itself via `i128::from`,
// but the bounds are already `i128`. Skip the conversion entirely
// and clamp in `i128`.
#[cfg(feature = "proptest")]
impl ArbitraryNumeric for i128 {
    type RangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        use proptest::strategy::Strategy as _;
        edge_biased_range!(min, max)
    }
}

// `usize` / `isize` widen through their architecture-specific size.
// On every supported platform `<int>::BITS <= 64`, so the i128
// bounds fit comfortably; clamp through that.
#[cfg(feature = "proptest")]
impl ArbitraryNumeric for usize {
    type RangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        use proptest::strategy::Strategy as _;
        let ty_min: i128 = 0;
        let ty_max: i128 = i128::from(u64::MAX);
        let lo = if min < ty_min { ty_min } else { min };
        let hi = if max > ty_max { ty_max } else { max };
        // `lo, hi` are clamped to `[0, u64::MAX]`; both fit usize
        // on every supported platform (BITS <= 64 enforced
        // elsewhere).
        let lo = Self::try_from(lo).unwrap_or(0);
        let hi = Self::try_from(hi).unwrap_or(Self::MAX);
        edge_biased_range!(lo, hi)
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryNumeric for isize {
    type RangeStrategy = proptest::strategy::BoxedStrategy<Self>;

    #[inline]
    fn arbitrary_in_range(min: i128, max: i128) -> Self::RangeStrategy {
        use proptest::strategy::Strategy as _;
        let ty_min: i128 = i128::from(i64::MIN);
        let ty_max: i128 = i128::from(i64::MAX);
        let lo = if min < ty_min { ty_min } else { min };
        let hi = if max > ty_max { ty_max } else { max };
        let lo = Self::try_from(lo).unwrap_or(Self::MIN);
        let hi = Self::try_from(hi).unwrap_or(Self::MAX);
        edge_biased_range!(lo, hi)
    }
}

// ─── Rule impls. ──────────────────────────────────────────────────
//
// `Within<MIN, MAX>` is a nominal newtype that delegates to the
// internal `And<AtLeast<MIN>, AtMost<MAX>>` composition. Both inner
// rules share `NumericError`, so the composition's error is
// `NumericError` directly — no flattening shim is needed.

macro_rules! const_within_constructor {
    ($(#[$doc:meta])* $method:ident, $ty:ty) => {
        $(#[$doc])*
        ///
        /// # Errors
        ///
        /// Returns [`NumericError::OutOfRange`] when `raw` is
        /// outside the inclusive range.
        #[inline]
        pub const fn $method(raw: $ty) -> Result<Refined<$ty, Self>, NumericError> {
            const { Self::VALID };
            let widened = raw as i128;
            if widened < MIN || widened > MAX {
                Err(NumericError::OutOfRange { value: widened })
            } else {
                Ok(Refined::from_inner(raw))
            }
        }
    };
}

impl<const MIN: i128, const MAX: i128> Within<MIN, MAX> {
    /// Single source of the bound invariant: `MIN <= MAX`. Referenced
    /// from `Rule::refine` and `ArbitraryRule::arbitrary_strategy`
    /// via `const { Self::VALID }`.
    const VALID: () = assert!(MIN <= MAX, "Within: MIN must be <= MAX");

    const_within_constructor!(
        /// Const-capable construction for `i8` carriers.
        ///
        /// This is the literal-friendly counterpart to
        /// `Refined::<i8, Within<MIN, MAX>>::try_new`: the same
        /// range predicate is checked in a `const fn`, so known
        /// protocol constants can be represented as refined values
        /// without a runtime `unwrap`.
        ///
        /// # Errors
        ///
        /// Returns [`NumericError::OutOfRange`] when `raw` is
        /// outside the inclusive range.
        ///
        /// # Examples
        ///
        /// ```
        /// use whittle_core::Refined;
        /// use whittle_core::primitive::Within;
        ///
        /// const OK: Refined<i8, Within<-10, 10>> =
        ///     match Within::<-10, 10>::try_new_i8(7) {
        ///         Ok(value) => value,
        ///         Err(_) => panic!("invalid literal"),
        ///     };
        ///
        /// assert_eq!(*OK.as_inner(), 7);
        /// ```
        try_new_i8,
        i8
    );

    const_within_constructor!(
        /// Const-capable construction for `i16` carriers.
        try_new_i16,
        i16
    );

    const_within_constructor!(
        /// Const-capable construction for `i32` carriers.
        try_new_i32,
        i32
    );

    const_within_constructor!(
        /// Const-capable construction for `i64` carriers.
        try_new_i64,
        i64
    );

    const_within_constructor!(
        /// Const-capable construction for `i128` carriers.
        try_new_i128,
        i128
    );

    const_within_constructor!(
        /// Const-capable construction for `isize` carriers.
        try_new_isize,
        isize
    );

    const_within_constructor!(
        /// Const-capable construction for `u8` carriers.
        try_new_u8,
        u8
    );

    const_within_constructor!(
        /// Const-capable construction for `u16` carriers.
        try_new_u16,
        u16
    );

    const_within_constructor!(
        /// Const-capable construction for `u32` carriers.
        try_new_u32,
        u32
    );

    const_within_constructor!(
        /// Const-capable construction for `u64` carriers.
        try_new_u64,
        u64
    );

    const_within_constructor!(
        /// Const-capable construction for `usize` carriers.
        try_new_usize,
        usize
    );
}

impl<T, const MIN: i128, const MAX: i128> Rule<T> for Within<MIN, MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        const { Self::VALID };
        <crate::composition::And<AtLeast<MIN>, AtMost<MAX>> as Rule<T>>::refine(raw)
    }
}

impl<T, const MIN: i128> Rule<T> for AtLeast<MIN>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened < MIN {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T, const MAX: i128> Rule<T> for AtMost<MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened > MAX {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<const MIN: i128> GreaterThan<MIN> {
    /// Single source of the bound invariant: `MIN < i128::MAX` so
    /// the strategy's `MIN + 1` never overflows. Referenced from
    /// `Rule::refine` and `ArbitraryRule::arbitrary_strategy` via
    /// `const { Self::VALID }`.
    const VALID: () = assert!(
        MIN < i128::MAX,
        "GreaterThan: MIN must be less than i128::MAX",
    );
}

impl<T, const MIN: i128> Rule<T> for GreaterThan<MIN>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        const { Self::VALID };
        let widened = raw.into_i128();
        if widened <= MIN {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<const MAX: i128> LessThan<MAX> {
    /// Single source of the bound invariant: `MAX > i128::MIN` so
    /// the strategy's `MAX - 1` never underflows. Referenced from
    /// `Rule::refine` and `ArbitraryRule::arbitrary_strategy` via
    /// `const { Self::VALID }`.
    const VALID: () = assert!(
        MAX > i128::MIN,
        "LessThan: MAX must be greater than i128::MIN",
    );
}

impl<T, const MAX: i128> Rule<T> for LessThan<MAX>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        const { Self::VALID };
        let widened = raw.into_i128();
        if widened >= MAX {
            return Err(NumericError::OutOfRange { value: widened });
        }
        T::from_i128(widened)
    }
}

impl<T, const N: i128> Rule<T> for EqualTo<N>
where
    T: Numeric,
{
    type Error = NumericError;

    #[inline]
    fn refine(raw: T) -> Result<T, Self::Error> {
        let widened = raw.into_i128();
        if widened == N {
            T::from_i128(widened)
        } else {
            Err(NumericError::OutOfRange { value: widened })
        }
    }
}

// `NotEqualTo<N>` is `Not<EqualTo<N>>`; its `Rule` impl comes from
// the generic `Not<R>` impl in `composition.rs`.
//
// `NonZero`, `Positive`, and `Negative` are type aliases for
// `NotEqualTo<0>`, `GreaterThan<0>`, and `LessThan<0>` respectively;
// their `Rule` and `ArbitraryRule` impls come from the underlying
// generic impls above.

// ─── Serde `DeserializeRule` impls: default parse-then-refine. ────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const MIN: i128, const MAX: i128] DeserializeRule<T> for Within<MIN, MAX>
    where [T: Numeric]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const MIN: i128] DeserializeRule<T> for AtLeast<MIN>
    where [T: Numeric]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const MAX: i128] DeserializeRule<T> for AtMost<MAX>
    where [T: Numeric]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const MIN: i128] DeserializeRule<T> for GreaterThan<MIN>
    where [T: Numeric]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const MAX: i128] DeserializeRule<T> for LessThan<MAX>
    where [T: Numeric]
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[T, const N: i128] DeserializeRule<T> for EqualTo<N>
    where [T: Numeric]
}

// ─── `SchemaRule` impls. ──────────────────────────────────────────
//
// Each schema reads the SAME const generics `refine` reads — the
// bound itself is the single determinant — and is interpreted within
// the carrier's embedding (`AtMost<300>` over `u8` still describes
// the admitted set exactly; values above `u8::MAX` are outside the
// embedding). Open bounds normalise to the adjacent inclusive bound,
// exactly as `refine`'s `<`/`>` comparisons admit them.

impl<T, const MIN: i128, const MAX: i128> SchemaRule<T> for Within<MIN, MAX>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        const { Self::VALID };
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(MIN)),
            Bound::Inclusive(Scalar::Int(MAX)),
        )
    }
}

impl<T, const MIN: i128> SchemaRule<T> for AtLeast<MIN>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(MIN)),
            Bound::Unbounded,
        )
    }
}

impl<T, const MAX: i128> SchemaRule<T> for AtMost<MAX>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(MAX)),
        )
    }
}

impl<T, const MIN: i128> SchemaRule<T> for GreaterThan<MIN>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        const { Self::VALID };
        // `MIN + 1` is the smallest admitted integer; VALID
        // guarantees the addition does not overflow (the same
        // invariant the strategy relies on).
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(MIN + 1)),
            Bound::Unbounded,
        )
    }
}

impl<T, const MAX: i128> SchemaRule<T> for LessThan<MAX>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        const { Self::VALID };
        // `MAX - 1` is the largest admitted integer; VALID
        // guarantees the subtraction does not underflow.
        Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(MAX - 1)),
        )
    }
}

impl<T, const N: i128> SchemaRule<T> for EqualTo<N>
where
    T: Numeric,
{
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(N)),
            Bound::Inclusive(Scalar::Int(N)),
        )
    }
}

// `NotEqualTo<N>` is `Not<EqualTo<N>>`: the complement of a point is
// the union of the two adjacent half-bounded intervals. At an i128
// extreme one side is empty and the union collapses to the single
// remaining interval. The bounds mirror `Not<R>`'s `Rule` impl
// (`T: Numeric + Copy`, operand error `NumericError`).
impl<T, const N: i128> SchemaRule<T> for crate::composition::Not<EqualTo<N>>
where
    T: Numeric + Copy,
{
    #[inline]
    fn schema() -> Schema {
        point_complement_schema(N)
    }
}

/// The complement of the single integer `point`: the union of the
/// two adjacent half-bounded intervals, with an empty side dropped
/// at the `i128` extremes. Non-generic so every `NotEqualTo<N>`
/// instantiation shares one function (the per-`N` branches could
/// never both be taken inside a single monomorphisation).
fn point_complement_schema(point: i128) -> Schema {
    let mut members = alloc::vec::Vec::with_capacity(2);
    if point > i128::MIN {
        members.push(Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(point - 1)),
        ));
    }
    if point < i128::MAX {
        members.push(Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(point + 1)),
            Bound::Unbounded,
        ));
    }
    Schema::union(members)
}

// ─── `ArbitraryRule` impls. ───────────────────────────────────────
//
// Each rule's strategy emits values that are admissible by
// construction: a `[min, max]` window clamped to the target type's
// range, plus a `prop_filter` for `NonZero` (where the admissible
// region is dense and the rejection-sampling cost is one in
// ~2^N).

#[cfg(feature = "proptest")]
impl<T, const MIN: i128, const MAX: i128> ArbitraryRule<T> for Within<MIN, MAX>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        T::arbitrary_in_range(MIN, MAX).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, const MIN: i128> ArbitraryRule<T> for AtLeast<MIN>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        T::arbitrary_in_range(MIN, i128::MAX).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, const MAX: i128> ArbitraryRule<T> for AtMost<MAX>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        T::arbitrary_in_range(i128::MIN, MAX).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, const MIN: i128> ArbitraryRule<T> for GreaterThan<MIN>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        // `MIN + 1` is the smallest admissible value; VALID
        // guarantees `MIN < i128::MAX` so the addition does not
        // overflow.
        T::arbitrary_in_range(MIN + 1, i128::MAX).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, const MAX: i128> ArbitraryRule<T> for LessThan<MAX>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        const { Self::VALID };
        // `MAX - 1` is the largest admissible value; VALID
        // guarantees `MAX > i128::MIN` so the subtraction does not
        // underflow.
        T::arbitrary_in_range(i128::MIN, MAX - 1).boxed()
    }
}

#[cfg(feature = "proptest")]
impl<T, const N: i128> ArbitraryRule<T> for EqualTo<N>
where
    T: ArbitraryNumeric + core::fmt::Debug,
{
    type Strategy = proptest::strategy::BoxedStrategy<T>;

    /// `EqualTo<N>` admits exactly one value: `N` rendered in `T`.
    /// Panics at strategy construction if `N` is outside `T`'s
    /// representable range — a programming error caught at test
    /// time.
    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let value: T = T::from_i128(N).expect("EqualTo<N>: N must fit in the carrier type T");
        proptest::strategy::Just(value).boxed()
    }
}

// `NotEqualTo<N>`'s `ArbitraryRule` impl comes from the generic
// `Not<R>` impl in `composition.rs`.

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString;

    use super::{
        AtLeast, AtMost, EqualTo, GreaterThan, LessThan, Negative, NonZero, NotEqualTo,
        NumericError, Positive, Within,
    };
    use crate::rule::{Refined, Rule};

    refinement! {
        /// Macro-generated newtype for testing: `i32` in `0..=100`.
        ///
        /// Exists to exercise `refinement!` from the numeric test
        /// module so the macro is reached by more than just
        /// `macros.rs`'s own tests.
        #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
        pub Percent: i32, Within<0, 100>;
    }

    #[test]
    fn within_accepts_bounds_inclusive() {
        let zero: Refined<i32, Within<0, 100>> = Refined::try_new(0_i32).unwrap();
        let hundred: Refined<i32, Within<0, 100>> = Refined::try_new(100_i32).unwrap();
        assert_eq!(*zero.as_inner(), 0_i32);
        assert_eq!(*hundred.as_inner(), 100_i32);
    }

    #[test]
    fn within_try_new_u16_constructs_const_refined_literal() {
        const OK: Refined<u16, Within<100, 599>> = match Within::<100, 599>::try_new_u16(200) {
            Ok(value) => value,
            Err(_) => panic!("200 is a valid HTTP status code"),
        };

        assert_eq!(*OK.as_inner(), 200_u16);
    }

    #[test]
    fn within_const_constructors_accept_supported_numeric_carriers() {
        const I8: Refined<i8, Within<-10, 10>> = match Within::<-10, 10>::try_new_i8(7) {
            Ok(value) => value,
            Err(_) => panic!("valid i8 literal"),
        };
        const I16: Refined<i16, Within<-10, 10>> = match Within::<-10, 10>::try_new_i16(7) {
            Ok(value) => value,
            Err(_) => panic!("valid i16 literal"),
        };
        const I32: Refined<i32, Within<-10, 10>> = match Within::<-10, 10>::try_new_i32(7) {
            Ok(value) => value,
            Err(_) => panic!("valid i32 literal"),
        };
        const I64: Refined<i64, Within<-10, 10>> = match Within::<-10, 10>::try_new_i64(7) {
            Ok(value) => value,
            Err(_) => panic!("valid i64 literal"),
        };
        const I128: Refined<i128, Within<-10, 10>> = match Within::<-10, 10>::try_new_i128(7) {
            Ok(value) => value,
            Err(_) => panic!("valid i128 literal"),
        };
        const ISIZE: Refined<isize, Within<-10, 10>> = match Within::<-10, 10>::try_new_isize(7) {
            Ok(value) => value,
            Err(_) => panic!("valid isize literal"),
        };
        const U8: Refined<u8, Within<0, 10>> = match Within::<0, 10>::try_new_u8(7) {
            Ok(value) => value,
            Err(_) => panic!("valid u8 literal"),
        };
        const U32: Refined<u32, Within<0, 10>> = match Within::<0, 10>::try_new_u32(7) {
            Ok(value) => value,
            Err(_) => panic!("valid u32 literal"),
        };
        const U64: Refined<u64, Within<0, 10>> = match Within::<0, 10>::try_new_u64(7) {
            Ok(value) => value,
            Err(_) => panic!("valid u64 literal"),
        };
        const USIZE: Refined<usize, Within<0, 10>> = match Within::<0, 10>::try_new_usize(7) {
            Ok(value) => value,
            Err(_) => panic!("valid usize literal"),
        };

        assert_eq!(*I8.as_inner(), 7_i8);
        assert_eq!(*I16.as_inner(), 7_i16);
        assert_eq!(*I32.as_inner(), 7_i32);
        assert_eq!(*I64.as_inner(), 7_i64);
        assert_eq!(*I128.as_inner(), 7_i128);
        assert_eq!(*ISIZE.as_inner(), 7_isize);
        assert_eq!(*U8.as_inner(), 7_u8);
        assert_eq!(*U32.as_inner(), 7_u32);
        assert_eq!(*U64.as_inner(), 7_u64);
        assert_eq!(*USIZE.as_inner(), 7_usize);
    }

    #[test]
    fn within_const_constructors_reject_supported_numeric_carriers() {
        assert_eq!(
            Within::<-10, 10>::try_new_i8(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<-10, 10>::try_new_i16(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<-10, 10>::try_new_i32(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<-10, 10>::try_new_i64(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<-10, 10>::try_new_i128(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<-10, 10>::try_new_isize(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<0, 10>::try_new_u8(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<0, 10>::try_new_u32(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<0, 10>::try_new_u64(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
        assert_eq!(
            Within::<0, 10>::try_new_usize(11).unwrap_err(),
            NumericError::OutOfRange { value: 11 },
        );
    }

    #[test]
    fn within_try_new_u16_accepts_at_runtime_for_coverage() {
        let ok = Within::<100, 599>::try_new_u16(200).unwrap();
        assert_eq!(*ok.as_inner(), 200_u16);
    }

    #[test]
    fn within_try_new_u16_rejects_out_of_range() {
        const ERR: Result<Refined<u16, Within<100, 599>>, NumericError> =
            Within::<100, 599>::try_new_u16(99);

        assert_eq!(ERR.unwrap_err(), NumericError::OutOfRange { value: 99 });
    }

    #[test]
    fn within_try_new_u16_rejects_at_runtime_for_coverage() {
        let err = Within::<100, 599>::try_new_u16(99).unwrap_err();
        assert_eq!(err, NumericError::OutOfRange { value: 99 });
    }

    #[test]
    fn within_rejects_out_of_range() {
        // Both inner rules of `Within`'s composition share
        // `NumericError`, so the domain error surfaces directly for
        // both sides — no flattening shim required.
        let neg: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(-1_i32);
        assert_eq!(
            neg.unwrap_err(),
            NumericError::OutOfRange { value: -1_i128 },
        );
        let big: Result<Refined<i32, Within<0, 100>>, _> = Refined::try_new(101_i32);
        assert_eq!(
            big.unwrap_err(),
            NumericError::OutOfRange { value: 101_i128 },
        );
    }

    #[test]
    fn at_least_at_most_compose_via_within() {
        // The primitives are independent; chained checks are demonstrated
        // here through manual sequencing.
        let inside = <AtLeast<5> as Rule<i32>>::refine(7_i32).unwrap();
        let inside = <AtMost<10> as Rule<i32>>::refine(inside).unwrap();
        assert_eq!(inside, 7_i32);
    }

    #[test]
    fn non_zero_rejects_zero_and_accepts_nonzero() {
        let result: Result<Refined<i32, NonZero>, _> = Refined::try_new(0_i32);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 0_i128 }
        );
        let accept: Refined<i32, NonZero> = Refined::try_new(-3_i32).unwrap();
        assert_eq!(*accept.as_inner(), -3_i32);
    }

    #[test]
    fn greater_than_admits_one_above_bound_and_rejects_at_bound() {
        let above: Refined<i32, GreaterThan<10>> = Refined::try_new(11_i32).unwrap();
        assert_eq!(*above.as_inner(), 11_i32);

        let at_bound: Result<Refined<i32, GreaterThan<10>>, _> = Refined::try_new(10_i32);
        assert_eq!(
            at_bound.unwrap_err(),
            NumericError::OutOfRange { value: 10_i128 },
        );

        let below: Result<Refined<i32, GreaterThan<10>>, _> = Refined::try_new(9_i32);
        assert_eq!(
            below.unwrap_err(),
            NumericError::OutOfRange { value: 9_i128 },
        );
    }

    #[test]
    fn less_than_admits_one_below_bound_and_rejects_at_bound() {
        let below: Refined<i32, LessThan<100>> = Refined::try_new(99_i32).unwrap();
        assert_eq!(*below.as_inner(), 99_i32);

        let at_bound: Result<Refined<i32, LessThan<100>>, _> = Refined::try_new(100_i32);
        assert_eq!(
            at_bound.unwrap_err(),
            NumericError::OutOfRange { value: 100_i128 },
        );

        let above: Result<Refined<i32, LessThan<100>>, _> = Refined::try_new(101_i32);
        assert_eq!(
            above.unwrap_err(),
            NumericError::OutOfRange { value: 101_i128 },
        );
    }

    #[test]
    fn open_bounds_work_for_unsigned_types() {
        let ok: Refined<u32, GreaterThan<0>> = Refined::try_new(1_u32).unwrap();
        assert_eq!(*ok.as_inner(), 1_u32);

        let zero: Result<Refined<u32, GreaterThan<0>>, _> = Refined::try_new(0_u32);
        assert_eq!(
            zero.unwrap_err(),
            NumericError::OutOfRange { value: 0_i128 },
        );
    }

    #[test]
    fn open_bounds_compose_with_each_other_via_and() {
        // `And<GreaterThan<MIN>, LessThan<MAX>>` is the open-open
        // range — the equivalent of PostgreSQL's `(MIN, MAX)`.
        use crate::And;
        type OpenOpen = And<GreaterThan<0>, LessThan<10>>;
        let mid: Refined<i32, OpenOpen> = Refined::try_new(5_i32).unwrap();
        assert_eq!(*mid.as_inner(), 5_i32);

        let zero: Result<Refined<i32, OpenOpen>, _> = Refined::try_new(0_i32);
        zero.unwrap_err();
        let ten: Result<Refined<i32, OpenOpen>, _> = Refined::try_new(10_i32);
        ten.unwrap_err();
    }

    #[test]
    fn positive_negative_partition() {
        let p: Refined<i32, Positive> = Refined::try_new(1_i32).unwrap();
        let n: Refined<i32, Negative> = Refined::try_new(-1_i32).unwrap();
        assert_eq!(*p.as_inner(), 1_i32);
        assert_eq!(*n.as_inner(), -1_i32);

        let p_zero: Result<Refined<i32, Positive>, _> = Refined::try_new(0_i32);
        p_zero.unwrap_err();
        let n_zero: Result<Refined<i32, Negative>, _> = Refined::try_new(0_i32);
        n_zero.unwrap_err();
    }

    #[test]
    fn within_works_for_unsigned_types() {
        let v: Refined<u8, Within<0, 100>> = Refined::try_new(42_u8).unwrap();
        assert_eq!(*v.as_inner(), 42_u8);
    }

    #[test]
    fn out_of_range_for_narrower_type_reports_underlying_value() {
        // i8 narrowing to fit i128 of 200 fails — 200 > i8::MAX.
        let result = <i8 as super::Numeric>::from_i128(200_i128);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    // ─── Numeric trait coverage for every integer impl. ──────────
    //
    // Each Numeric impl is its own monomorphization, so we round-trip
    // every variant through Within to exercise both `into_i128` and
    // `from_i128`. The cases here are deliberately minimal: a single
    // admissible round-trip per type is enough to take all branches.

    #[test]
    fn within_round_trip_i16() {
        let v: Refined<i16, Within<-100, 100>> = Refined::try_new(42_i16).unwrap();
        assert_eq!(*v.as_inner(), 42_i16);
    }

    #[test]
    fn within_round_trip_i64() {
        let v: Refined<i64, Within<-100, 100>> = Refined::try_new(42_i64).unwrap();
        assert_eq!(*v.as_inner(), 42_i64);
    }

    #[test]
    fn within_round_trip_u16() {
        let v: Refined<u16, Within<0, 100>> = Refined::try_new(42_u16).unwrap();
        assert_eq!(*v.as_inner(), 42_u16);
    }

    #[test]
    fn within_round_trip_u32() {
        let v: Refined<u32, Within<0, 100>> = Refined::try_new(42_u32).unwrap();
        assert_eq!(*v.as_inner(), 42_u32);
    }

    #[test]
    fn within_round_trip_u64() {
        let v: Refined<u64, Within<0, 100>> = Refined::try_new(42_u64).unwrap();
        assert_eq!(*v.as_inner(), 42_u64);
    }

    #[test]
    fn within_round_trip_i128() {
        let v: Refined<i128, Within<-100, 100>> = Refined::try_new(42_i128).unwrap();
        assert_eq!(*v.as_inner(), 42_i128);
    }

    #[test]
    fn within_round_trip_usize() {
        let v: Refined<usize, Within<0, 100>> = Refined::try_new(42_usize).unwrap();
        assert_eq!(*v.as_inner(), 42_usize);
    }

    #[test]
    fn within_round_trip_isize() {
        let v: Refined<isize, Within<-100, 100>> = Refined::try_new(42_isize).unwrap();
        assert_eq!(*v.as_inner(), 42_isize);
    }

    // Failure-path coverage for the from_i128 narrowing branch on
    // usize and isize. (i128 cannot fail conversion; its from_i128
    // is Ok((self)).)
    #[test]
    fn within_rejects_overflow_for_usize() {
        let result: Result<Refined<usize, Within<0, 100>>, _> = Refined::try_new(200_usize);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    #[test]
    fn within_rejects_overflow_for_isize() {
        let result: Result<Refined<isize, Within<-100, 100>>, _> = Refined::try_new(200_isize);
        assert_eq!(
            result.unwrap_err(),
            NumericError::OutOfRange { value: 200_i128 },
        );
    }

    // `Numeric::from_i128` is a per-type entry point reached not
    // only through `Within::refine` but also as part of the wider
    // `Rule` surface; covering its narrowing failure branch
    // requires a value that fits in i128 but not in the target.
    //
    // Within<MIN, MAX>::refine clamps before calling from_i128, so
    // the closure is only reached when the value passes the
    // refinement check but is then out of the target's range. We
    // call from_i128 directly here to exercise each impl's
    // narrowing-error closure once.

    #[test]
    fn within_round_trip_i8() {
        // Exercises i8's into_i128 + Within::refine monomorphization.
        let v: Refined<i8, Within<-100, 100>> = Refined::try_new(42_i8).unwrap();
        assert_eq!(*v.as_inner(), 42_i8);
    }

    fn assert_from_i128_overflow<T: super::Numeric>(value: i128) {
        let result = <T as super::Numeric>::from_i128(value);
        assert_eq!(result.err(), Some(NumericError::OutOfRange { value }),);
    }

    #[test]
    fn from_i128_overflow_signed() {
        // Each macro-generated signed impl gets its narrowing
        // closure exercised once.
        assert_from_i128_overflow::<i8>(200_i128);
        assert_from_i128_overflow::<i16>(40_000_i128);
        assert_from_i128_overflow::<i32>(3_000_000_000_i128);
        assert_from_i128_overflow::<i64>(i128::MAX);
    }

    #[test]
    fn from_i128_overflow_unsigned() {
        // Each macro-generated unsigned impl. Negative values are
        // always out of range for unsigned types.
        assert_from_i128_overflow::<u8>(-1_i128);
        assert_from_i128_overflow::<u16>(-1_i128);
        assert_from_i128_overflow::<u32>(-1_i128);
        assert_from_i128_overflow::<u64>(-1_i128);
    }

    #[test]
    fn from_i128_overflow_pointer_sized() {
        assert_from_i128_overflow::<usize>(-1_i128);
        assert_from_i128_overflow::<isize>(i128::MAX);
    }

    #[test]
    fn display_formats_out_of_range_variant() {
        // Exercise the hand-rolled `Display` arm for the only
        // `NumericError` variant. Pairing with `core::error::Error`
        // via the `dyn Error` cast confirms the trait impl is live.
        let err = NumericError::OutOfRange { value: -7_i128 };
        assert_eq!(err.to_string(), "value -7 not in admissible range");
        let dyn_err: &dyn core::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn refinement_macro_percent_admits_and_rejects() {
        // Exercises the macro-generated newtype: admit a mid-range
        // value, reject above MAX. Confirms `refinement!` reaches the
        // numeric primitive test module.
        let ok = Percent::try_new(42_i32).unwrap();
        assert_eq!(*ok.as_inner(), 42_i32);
        let owned: i32 = ok.into_inner();
        assert_eq!(owned, 42_i32);
        let bad = Percent::try_new(101_i32);
        bad.unwrap_err();
    }

    proptest::proptest! {
        #[test]
        fn within_round_trips_admissible(x in 0_i32..=100_i32) {
            let r: Refined<i32, Within<0, 100>> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

        #[test]
        fn within_rejects_below_min(x in i32::MIN..0_i32) {
            // kernel-only: domain code wraps this composition in a
            // newtype with a flat error enum — see SKILL.md
            // "Newtype hiding rule composition".
            let result: Result<Refined<i32, Within<0, 100>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_least_rejects_below_min(x in i32::MIN..10_i32) {
            let result: Result<Refined<i32, AtLeast<10>>, _> = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn non_zero_round_trips_nonzero(x in proptest::arbitrary::any::<i32>()) {
            proptest::prop_assume!(x != 0_i32);
            let r: Refined<i32, NonZero> = Refined::try_new(x).unwrap();
            proptest::prop_assert_eq!(*r.as_inner(), x);
        }

    }

    // ─── Self-hosted Arbitrary round-trips. Every value
    //     generated by the `Refined<T, R>` Arbitrary strategy
    //     must satisfy `R` by construction.

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_within_is_in_range(
            r in proptest::arbitrary::any::<Refined<i32, Within<0, 100>>>()
        ) {
            // `Within<0, 100>`'s `ArbitraryRule` strategy emits
            // values directly in `[0, 100]`, so the carrier is
            // admissible by construction without rejection
            // sampling against the full `i32` range.
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_at_least_is_above_min(
            r in proptest::arbitrary::any::<Refined<i32, AtLeast<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() >= 10);
        }

        #[test]
        fn arbitrary_at_most_is_below_max(
            r in proptest::arbitrary::any::<Refined<i32, AtMost<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() <= 10);
        }

        #[test]
        fn arbitrary_greater_than_is_strictly_above_min(
            r in proptest::arbitrary::any::<Refined<i32, GreaterThan<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() > 10);
        }

        #[test]
        fn arbitrary_less_than_is_strictly_below_max(
            r in proptest::arbitrary::any::<Refined<i32, LessThan<10>>>()
        ) {
            proptest::prop_assert!(*r.as_inner() < 10);
        }

        #[test]
        fn arbitrary_non_zero_is_nonzero(
            r in proptest::arbitrary::any::<Refined<i32, NonZero>>()
        ) {
            proptest::prop_assert!(*r.as_inner() != 0);
        }

        #[test]
        fn arbitrary_positive_is_positive(
            r in proptest::arbitrary::any::<Refined<i32, Positive>>()
        ) {
            proptest::prop_assert!(*r.as_inner() > 0);
        }

        #[test]
        fn arbitrary_negative_is_negative(
            r in proptest::arbitrary::any::<Refined<i32, Negative>>()
        ) {
            proptest::prop_assert!(*r.as_inner() < 0);
        }

        #[test]
        fn arbitrary_equal_to_is_exactly_n(
            r in proptest::arbitrary::any::<Refined<i32, super::EqualTo<42>>>()
        ) {
            // `EqualTo<N>`'s strategy is `Just(N)`; the single
            // admissible value is `N` rendered in the carrier.
            proptest::prop_assert_eq!(*r.as_inner(), 42);
        }
    }

    proptest::proptest! {
        // ─── Reject properties: bounded ranges. ────────────────

        #[test]
        fn within_rejects_strictly_above_max(x in 101_i32..=i32::MAX) {
            let result: Result<Refined<i32, Within<0, 100>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_least_rejects_strictly_below_min_band(
            x in i32::MIN..10_i32
        ) {
            let result: Result<Refined<i32, AtLeast<10>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

        #[test]
        fn at_most_rejects_strictly_above_max_band(
            x in 11_i32..=i32::MAX
        ) {
            let result: Result<Refined<i32, AtMost<10>>, _>
                = Refined::try_new(x);
            proptest::prop_assert_eq!(
                result.unwrap_err(),
                NumericError::OutOfRange { value: i128::from(x) },
            );
        }

    }

    // ─── `ArbitraryNumeric` impls for every supported integer
    //     type. Each Within strategy is its own
    //     monomorphisation; touching one per type pins the
    //     branch inside `arbitrary_in_range` to the coverage
    //     graph.

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_within_round_trips_i8(
            r in proptest::arbitrary::any::<Refined<i8, Within<-50, 50>>>()
        ) {
            proptest::prop_assert!((-50..=50).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_i16(
            r in proptest::arbitrary::any::<Refined<i16, Within<-100, 100>>>()
        ) {
            proptest::prop_assert!((-100..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_i64(
            r in proptest::arbitrary::any::<Refined<i64, Within<-100, 100>>>()
        ) {
            proptest::prop_assert!((-100..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_i128(
            r in proptest::arbitrary::any::<Refined<i128, Within<-100, 100>>>()
        ) {
            proptest::prop_assert!((-100..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_u8(
            r in proptest::arbitrary::any::<Refined<u8, Within<0, 100>>>()
        ) {
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_u16(
            r in proptest::arbitrary::any::<Refined<u16, Within<0, 100>>>()
        ) {
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_u32(
            r in proptest::arbitrary::any::<Refined<u32, Within<0, 100>>>()
        ) {
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_u64(
            r in proptest::arbitrary::any::<Refined<u64, Within<0, 100>>>()
        ) {
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_usize(
            r in proptest::arbitrary::any::<Refined<usize, Within<0, 100>>>()
        ) {
            proptest::prop_assert!((0..=100).contains(r.as_inner()));
        }

        #[test]
        fn arbitrary_within_round_trips_isize(
            r in proptest::arbitrary::any::<Refined<isize, Within<-100, 100>>>()
        ) {
            proptest::prop_assert!((-100..=100).contains(r.as_inner()));
        }
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn arbitrary_within_emits_both_endpoints() {
        // R-T3 edge bias: each clamped endpoint carries weight 1
        // against weight 8 for the interior, so a 256-draw sample
        // must contain both bounds. Deterministic runner — the
        // assertion can never flake on an unlucky seed.
        use proptest::strategy::{Strategy as _, ValueTree as _};
        let strategy = <Within<0, 100> as crate::rule::ArbitraryRule<i32>>::arbitrary_strategy();
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let mut saw_min = false;
        let mut saw_max = false;
        for _ in 0_u32..256 {
            let value = strategy.new_tree(&mut runner).unwrap().current();
            saw_min |= value == 0;
            saw_max |= value == 100;
        }
        assert!(saw_min, "edge-biased Within must emit MIN");
        assert!(saw_max, "edge-biased Within must emit MAX");
    }

    // ─── SchemaRule: the constructive descriptions. ────────────────
    //
    // Each schema must read the same const generics `refine` reads;
    // the structural asserts pin the shape, and the cross-checks
    // (behind `proptest`) are the mechanical oracle between the two
    // determinants.

    use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

    fn closed(lo: i128, hi: i128) -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(lo)),
            Bound::Inclusive(Scalar::Int(hi)),
        )
    }

    fn at_least(lo: i128) -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Inclusive(Scalar::Int(lo)),
            Bound::Unbounded,
        )
    }

    fn at_most(hi: i128) -> Schema {
        Schema::interval(
            ScalarKind::Integer,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(hi)),
        )
    }

    #[test]
    fn schema_reads_the_same_bounds_refine_reads() {
        // Two carrier instantiations per impl: the schema is a
        // property of the rule's const generics, not the carrier.
        assert_eq!(
            <Within<0, 100> as SchemaRule<i32>>::schema(),
            closed(0, 100)
        );
        assert_eq!(<Within<0, 100> as SchemaRule<u8>>::schema(), closed(0, 100));
        assert_eq!(<AtLeast<10> as SchemaRule<i32>>::schema(), at_least(10));
        assert_eq!(<AtLeast<10> as SchemaRule<i64>>::schema(), at_least(10));
        assert_eq!(<AtMost<10> as SchemaRule<i32>>::schema(), at_most(10));
        assert_eq!(<AtMost<10> as SchemaRule<u16>>::schema(), at_most(10));
        // Open bounds normalise to the adjacent inclusive bound.
        assert_eq!(<GreaterThan<10> as SchemaRule<i32>>::schema(), at_least(11));
        assert_eq!(<GreaterThan<10> as SchemaRule<u32>>::schema(), at_least(11));
        assert_eq!(<LessThan<100> as SchemaRule<i32>>::schema(), at_most(99));
        assert_eq!(<LessThan<100> as SchemaRule<i8>>::schema(), at_most(99));
        // The singleton rule is the degenerate interval.
        assert_eq!(<EqualTo<42> as SchemaRule<i32>>::schema(), closed(42, 42));
        assert_eq!(<EqualTo<42> as SchemaRule<u8>>::schema(), closed(42, 42));
    }

    #[test]
    fn schema_not_equal_to_is_the_two_interval_union() {
        let expected = Schema::union(alloc::vec![at_most(-1), at_least(1)]);
        assert_eq!(<NotEqualTo<0> as SchemaRule<i32>>::schema(), expected);
        assert_eq!(
            <NotEqualTo<0> as SchemaRule<i16>>::schema(),
            Schema::union(alloc::vec![at_most(-1), at_least(1)]),
        );
    }

    #[test]
    fn schema_not_equal_to_collapses_at_the_i128_extremes() {
        // The empty side of the complement drops out and the union
        // collapses to the single remaining interval.
        assert_eq!(
            <NotEqualTo<{ i128::MIN }> as SchemaRule<i128>>::schema(),
            at_least(i128::MIN + 1),
        );
        assert_eq!(
            <NotEqualTo<{ i128::MAX }> as SchemaRule<i128>>::schema(),
            at_most(i128::MAX - 1),
        );
    }

    #[test]
    fn schema_aliases_inherit_their_alias_definitions() {
        assert_eq!(<Positive as SchemaRule<i32>>::schema(), at_least(1));
        assert_eq!(<Negative as SchemaRule<i32>>::schema(), at_most(-1));
        assert_eq!(
            <NonZero as SchemaRule<i32>>::schema(),
            Schema::union(alloc::vec![at_most(-1), at_least(1)]),
        );
    }

    #[cfg(feature = "proptest")]
    mod schema_cross_checks {
        use super::super::{AtLeast, AtMost, EqualTo, GreaterThan, LessThan, NonZero, Within};
        use crate::schema::{Scalar, ScalarKind};
        use crate::testing::prop_schema_cross_check;

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
        )]
        fn embed_i32(value: &i32) -> (ScalarKind, Scalar) {
            (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
        }

        #[expect(
            clippy::return_and_then,
            reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                      would add a None arm no boundary candidate reaches"
        )]
        fn extract_i32(_kind: ScalarKind, scalar: Scalar) -> Option<i32> {
            scalar
                .as_int()
                .and_then(|widened| i32::try_from(widened).ok())
        }

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
        )]
        fn embed_u8(value: &u8) -> (ScalarKind, Scalar) {
            (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
        }

        #[expect(
            clippy::return_and_then,
            reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                      would add a None arm no boundary candidate reaches"
        )]
        fn extract_u8(_kind: ScalarKind, scalar: Scalar) -> Option<u8> {
            scalar
                .as_int()
                .and_then(|widened| u8::try_from(widened).ok())
        }

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "matches the helper's fn(&T) embedding signature over a generic carrier"
        )]
        fn embed_i16(value: &i16) -> (ScalarKind, Scalar) {
            (ScalarKind::Integer, Scalar::Int(i128::from(*value)))
        }

        #[expect(
            clippy::return_and_then,
            reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                      would add a None arm no boundary candidate reaches"
        )]
        fn extract_i16(_kind: ScalarKind, scalar: Scalar) -> Option<i16> {
            scalar
                .as_int()
                .and_then(|widened| i16::try_from(widened).ok())
        }

        /// Schema endpoints pass refine and strategy samples are
        /// schema members, for every numeric rule over `i32`.
        #[test]
        fn schema_cross_checks_numeric_rules_over_i32() {
            prop_schema_cross_check::<i32, Within<0, 100>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, AtLeast<10>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, AtMost<10>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, GreaterThan<10>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, LessThan<100>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, EqualTo<42>>(embed_i32, extract_i32);
            prop_schema_cross_check::<i32, NonZero>(embed_i32, extract_i32);
        }

        /// Second carrier monomorphisations per impl. `NonZero`'s
        /// schema endpoint `-1` needs a signed carrier, so it pairs
        /// with `i16` instead of `u8`.
        #[test]
        fn schema_cross_checks_numeric_rules_over_second_carriers() {
            prop_schema_cross_check::<u8, Within<0, 100>>(embed_u8, extract_u8);
            prop_schema_cross_check::<u8, AtLeast<10>>(embed_u8, extract_u8);
            prop_schema_cross_check::<u8, AtMost<10>>(embed_u8, extract_u8);
            prop_schema_cross_check::<u8, GreaterThan<10>>(embed_u8, extract_u8);
            prop_schema_cross_check::<u8, LessThan<100>>(embed_u8, extract_u8);
            prop_schema_cross_check::<u8, EqualTo<42>>(embed_u8, extract_u8);
            prop_schema_cross_check::<i16, NonZero>(embed_i16, extract_i16);
        }
    }

    #[cfg(feature = "proptest")]
    #[test]
    fn numeric_arbitrary_clamps_out_of_range_bounds() {
        // `arbitrary_in_range` clamps the requested `[min, max]`
        // bounds to the target type's representable range. Exercise
        // the clamping branches that the proptest-driven tests
        // above cannot reach for usize/isize (the `try_from`
        // fallbacks are intentionally unreachable through the
        // public rule surface).
        use super::ArbitraryNumeric;
        // Bind each strategy so the boxed values' destructors are
        // tied to a named binding rather than the temporary scope
        // (clippy's `let_underscore_drop` rejects `let _: T = ...`
        // when `T` has a destructor; the boxed strategies do).
        //
        // usize: lower bound clamped up from a negative i128.
        let _usize_low = usize::arbitrary_in_range(-1_i128, 10_i128);
        // usize: upper bound clamped down from beyond u64::MAX.
        let _usize_high = usize::arbitrary_in_range(0_i128, i128::from(u64::MAX) + 1);
        // isize: lower / upper bounds clamped to i64::MIN / i64::MAX.
        let _isize_bounds = isize::arbitrary_in_range(i128::MIN, i128::from(i64::MAX) + 1);
        // Macro-impl types: the explicit-cast branch when MIN/MAX
        // exceed the type's range.
        let _i8_full = i8::arbitrary_in_range(i128::MIN, i128::MAX);
        let _u8_full = u8::arbitrary_in_range(i128::MIN, i128::MAX);
    }
}
