//! Decimal primitive rules.
//!
//! Validation rules for `rust_decimal::Decimal`. Each primitive is a
//! `Rule<Decimal>` and returns the flat [`DecimalError`] enum.
//!
//! Available behind the `decimal` Cargo feature, which pulls in
//! `rust_decimal` as a dependency.
//!
//! Range bounds (`DecimalInRange`) are encoded as scaled `i128`
//! mantissas because Rust 2024 does not yet support `Decimal` const
//! generics — the same dodge `InClosedRange` uses for `f64`.

use rust_decimal::Decimal;

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// Maximum scale supported by `rust_decimal::Decimal`.
///
/// `Decimal::MAX_SCALE` is `u32` but every rule's scale parameter is
/// a `u8` const generic, so it is convenient to have the same bound
/// available as a `u8` for compile-time `const { assert!(...) }`
/// checks.
const DECIMAL_MAX_SCALE_U8: u8 = 28;

/// Maximum mantissa magnitude representable by `Decimal` (`2^96 - 1`).
///
/// `Decimal::from_i128_with_scale` panics when its `num` argument
/// exceeds this magnitude. `DecimalInRange` validates `MIN_REPR` and
/// `MAX_REPR` against this bound at compile time; proptest strategies
/// in this module sample mantissas only within this range.
const DECIMAL_MAX_MANTISSA: i128 = Decimal::MAX.mantissa();
/// Minimum mantissa magnitude representable by `Decimal`.
const DECIMAL_MIN_MANTISSA: i128 = Decimal::MIN.mantissa();

/// Flat domain error for decimal rules.
///
/// Each variant carries enough context to render a useful
/// diagnostic without leaking the rule's marker type. The variant
/// set is the union over all decimal rules in this module:
/// individual primitives only ever return a subset.
#[derive(Debug, PartialEq, Eq)]
pub enum DecimalError {
    /// Value was not strictly positive: `value <= 0`. Returned by
    /// [`DecimalPositive`]. Carries the offending value.
    NotPositive {
        /// Offending value.
        value: Decimal,
    },

    /// Value's decimal scale (digits after the point) did not match
    /// the rule's required scale. Returned by [`DecimalScale`].
    ScaleMismatch {
        /// Required scale.
        expected: u8,
        /// Scale that the value actually has.
        actual: u32,
    },

    /// Value's significant-digit count exceeded the rule's
    /// precision limit. Returned by [`DecimalPrecision`].
    PrecisionExceeded {
        /// Maximum admissible significant-digit count.
        limit: u8,
        /// Significant-digit count of the offending value.
        actual: u32,
    },

    /// Value is outside the admissible closed range. Returned by
    /// [`DecimalInRange`]. Carries the offending value.
    OutOfRange {
        /// Offending value.
        value: Decimal,
    },
}

impl core::fmt::Display for DecimalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::NotPositive { value } => {
                write!(f, "value {value} is not strictly positive")
            }
            Self::ScaleMismatch { expected, actual } => {
                write!(
                    f,
                    "value scale {actual} does not equal required scale {expected}"
                )
            }
            Self::PrecisionExceeded { limit, actual } => {
                write!(f, "value has {actual} significant digits; limit is {limit}")
            }
            Self::OutOfRange { value } => {
                write!(f, "value {value} out of admissible range")
            }
        }
    }
}

impl core::error::Error for DecimalError {}

/// Count significant digits in the unscaled mantissa.
///
/// Treats `0` as zero significant digits so `DecimalPrecision<P>`
/// admits `Decimal::ZERO` for every `P` — see the rule's docs.
#[inline]
const fn significant_digits(mantissa: i128) -> u32 {
    if mantissa == 0 {
        0
    } else {
        // `i128::unsigned_abs()` widens to `u128`; even
        // `i128::MIN.unsigned_abs()` is representable.
        mantissa.unsigned_abs().ilog10() + 1
    }
}

/// Reject non-positive values. Admit only `value > 0`.
///
/// `Decimal::ZERO` is rejected (zero is not strictly positive); use
/// a custom rule that admits zero when that is the contract.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "decimal")] {
/// use rust_decimal::Decimal;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DecimalError, DecimalPositive};
///
/// // Admit: 1.23 is strictly positive.
/// let ok: Refined<Decimal, DecimalPositive>
///     = Refined::try_new(Decimal::new(123, 2)).unwrap();
/// assert_eq!(*ok.as_inner(), Decimal::new(123, 2));
///
/// // Reject: zero is not strictly positive.
/// let err = Refined::<Decimal, DecimalPositive>::try_new(Decimal::ZERO)
///     .unwrap_err();
/// assert_eq!(err, DecimalError::NotPositive { value: Decimal::ZERO });
/// # }
/// ```
pub struct DecimalPositive;

impl Rule<Decimal> for DecimalPositive {
    type Error = DecimalError;

    #[inline]
    fn refine(raw: Decimal) -> Result<Decimal, Self::Error> {
        if raw > Decimal::ZERO {
            Ok(raw)
        } else {
            Err(DecimalError::NotPositive { value: raw })
        }
    }
}

/// Require a value's decimal scale to equal `S` exactly.
///
/// The scale is the count of digits after the decimal point in the
/// canonical representation. This rule is pure validation: callers
/// must rescale the input themselves before construction. The
/// strictness is deliberate — silently rewriting precision is risky
/// for monetary values.
///
/// `S` must be in `0..=28` (the maximum scale `rust_decimal`
/// supports). Values outside that range are caught at compile time
/// by a `const { assert!(...) }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "decimal")] {
/// use rust_decimal::Decimal;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DecimalError, DecimalScale};
///
/// // Admit: `9.99` has scale exactly 2.
/// let ok: Refined<Decimal, DecimalScale<2>>
///     = Refined::try_new(Decimal::new(999, 2)).unwrap();
/// assert_eq!(*ok.as_inner(), Decimal::new(999, 2));
///
/// // Reject: scale 0 is not scale 2.
/// let err = Refined::<Decimal, DecimalScale<2>>::try_new(Decimal::new(10, 0))
///     .unwrap_err();
/// assert_eq!(err, DecimalError::ScaleMismatch { expected: 2, actual: 0 });
/// # }
/// ```
pub struct DecimalScale<const S: u8>;

impl<const S: u8> DecimalScale<S> {
    const VALID: () = const {
        assert!(
            S <= DECIMAL_MAX_SCALE_U8,
            "DecimalScale<S>: S must be in 0..=28 (rust_decimal MAX_SCALE)",
        );
    };
}

impl<const S: u8> Rule<Decimal> for DecimalScale<S> {
    type Error = DecimalError;

    #[inline]
    fn refine(raw: Decimal) -> Result<Decimal, Self::Error> {
        let () = Self::VALID;
        let actual = raw.scale();
        if actual == u32::from(S) {
            Ok(raw)
        } else {
            Err(DecimalError::ScaleMismatch {
                expected: S,
                actual,
            })
        }
    }
}

/// Require a value's significant-digit count (precision) to be at
/// most `P`.
///
/// Precision is the count of significant digits in the canonical
/// unscaled mantissa, matching SQL `DECIMAL(P, S)` semantics:
///
/// - `Decimal::ZERO` is treated as 0 significant digits — admitted
///   for every `P`.
/// - `123.45` has precision 5 (the mantissa is `12345`).
/// - `0.001` has precision 1 (leading zeros do not count).
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "decimal")] {
/// use rust_decimal::Decimal;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DecimalError, DecimalPrecision};
///
/// // Admit: 12345 has 5 significant digits, within the limit.
/// let ok: Refined<Decimal, DecimalPrecision<5>>
///     = Refined::try_new(Decimal::new(12_345, 0)).unwrap();
/// assert_eq!(*ok.as_inner(), Decimal::new(12_345, 0));
///
/// // Reject: 123456 exceeds the limit of 5.
/// let err = Refined::<Decimal, DecimalPrecision<5>>::try_new(
///     Decimal::new(123_456, 0)
/// ).unwrap_err();
/// assert_eq!(err, DecimalError::PrecisionExceeded { limit: 5, actual: 6 });
/// # }
/// ```
pub struct DecimalPrecision<const P: u8>;

impl<const P: u8> Rule<Decimal> for DecimalPrecision<P> {
    type Error = DecimalError;

    #[inline]
    fn refine(raw: Decimal) -> Result<Decimal, Self::Error> {
        let actual = significant_digits(raw.mantissa());
        if actual <= u32::from(P) {
            Ok(raw)
        } else {
            Err(DecimalError::PrecisionExceeded { limit: P, actual })
        }
    }
}

/// Closed range `[MIN_REPR / 10^SCALE, MAX_REPR / 10^SCALE]`,
/// encoded as scaled `i128` mantissas.
///
/// Rust 2024 does not yet allow `Decimal` const generics, so the
/// admissible interval is encoded as a `(mantissa, scale)` ratio in
/// the same spirit as [`InClosedRange`] for `f64`:
///
/// - `DecimalInRange<0, 100, 0>` admits `0..=100` exactly.
/// - `DecimalInRange<0, 10_000, 2>` admits `0.00..=100.00` (the
///   upper bound is `10000 / 10^2 = 100.00`).
/// - `DecimalInRange<-99_999, 99_999, 3>` admits values in
///   `-99.999..=99.999`.
///
/// Bound values themselves remain admissible; the range is closed
/// at both ends. `SCALE` must be in `0..=28`. `MIN_REPR` must be
/// less than or equal to `MAX_REPR`. Both are checked at compile
/// time via `const { assert!(...) }`.
///
/// [`InClosedRange`]: crate::primitive::InClosedRange
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "decimal")] {
/// use rust_decimal::Decimal;
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DecimalError, DecimalInRange};
///
/// // Admit: 50.00 is within 0.00..=100.00.
/// type Pct = DecimalInRange<0, 10_000, 2>;
/// let ok: Refined<Decimal, Pct>
///     = Refined::try_new(Decimal::new(5_000, 2)).unwrap();
/// assert_eq!(*ok.as_inner(), Decimal::new(5_000, 2));
///
/// // Reject: 150.00 is above the upper bound.
/// let bad = Decimal::new(15_000, 2);
/// let err = Refined::<Decimal, Pct>::try_new(bad).unwrap_err();
/// assert_eq!(err, DecimalError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DecimalInRange<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8>;

impl<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8>
    DecimalInRange<MIN_REPR, MAX_REPR, SCALE>
{
    const VALID: () = const {
        assert!(
            SCALE <= DECIMAL_MAX_SCALE_U8,
            "DecimalInRange<MIN_REPR, MAX_REPR, SCALE>: SCALE must be in 0..=28 (rust_decimal MAX_SCALE)",
        );
        assert!(
            MIN_REPR <= MAX_REPR,
            "DecimalInRange<MIN_REPR, MAX_REPR, SCALE>: MIN_REPR must be <= MAX_REPR",
        );
        assert!(
            MIN_REPR >= DECIMAL_MIN_MANTISSA,
            "DecimalInRange<MIN_REPR, MAX_REPR, SCALE>: MIN_REPR below rust_decimal mantissa range",
        );
        assert!(
            MAX_REPR <= DECIMAL_MAX_MANTISSA,
            "DecimalInRange<MIN_REPR, MAX_REPR, SCALE>: MAX_REPR above rust_decimal mantissa range",
        );
    };
}

impl<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8> Rule<Decimal>
    for DecimalInRange<MIN_REPR, MAX_REPR, SCALE>
{
    type Error = DecimalError;

    #[inline]
    fn refine(raw: Decimal) -> Result<Decimal, Self::Error> {
        let () = Self::VALID;
        let scale = u32::from(SCALE);
        let lo = Decimal::from_i128_with_scale(MIN_REPR, scale);
        let hi = Decimal::from_i128_with_scale(MAX_REPR, scale);
        if raw >= lo && raw <= hi {
            Ok(raw)
        } else {
            Err(DecimalError::OutOfRange { value: raw })
        }
    }
}

/// Per-rule decimal strategy used by the blanket
/// `Refined<Decimal, R>: Arbitrary` impl.
///
/// Each rule supplies a strategy that targets the admissible region
/// directly — the carrier's `Arbitrary` maps the strategy through
/// `Refined::try_new`. The blanket impl does no rejection sampling;
/// rules over dense regions ([`DecimalPositive`]) use a single
/// `prop_filter` whose reject rate is negligible.
///
/// Available behind the `decimal` and `proptest` features.
#[cfg(feature = "proptest")]
pub trait ArbitraryDecimal: Rule<Decimal> {
    /// Concrete strategy type. Always resolves to a
    /// [`proptest::strategy::BoxedStrategy`] for API stability.
    type Strategy: proptest::strategy::Strategy<Value = Decimal>;

    /// Build the rule's `Decimal` strategy.
    fn arbitrary_decimal() -> Self::Strategy;
}

// ─── Serde `DeserializeRule` impls: default parse-then-refine.
//      Applicable only when `rust_decimal`'s own `serde` support is
//      enabled by the consumer (the `Decimal: Deserialize<'de>`
//      bound is satisfied through feature unification). ────────────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[] DeserializeRule<Decimal> for DecimalPositive
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const S: u8] DeserializeRule<Decimal> for DecimalScale<S>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const P: u8] DeserializeRule<Decimal> for DecimalPrecision<P>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8] DeserializeRule<Decimal>
    for DecimalInRange<MIN_REPR, MAX_REPR, SCALE>
}

#[cfg(feature = "proptest")]
impl<R: ArbitraryDecimal> ArbitraryRule<Decimal> for R {
    type Strategy = R::Strategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        R::arbitrary_decimal()
    }
}

#[cfg(feature = "proptest")]
impl ArbitraryDecimal for DecimalPositive {
    type Strategy = proptest::strategy::BoxedStrategy<Decimal>;

    #[inline]
    fn arbitrary_decimal() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // Construct strictly positive mantissas directly — no
        // rejection sampling against a 50% reject rate over the full
        // Decimal-representable range.
        (
            1_i128..=DECIMAL_MAX_MANTISSA,
            0_u32..=u32::from(DECIMAL_MAX_SCALE_U8),
        )
            .prop_map(|(m, s)| Decimal::from_i128_with_scale(m, s))
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const S: u8> ArbitraryDecimal for DecimalScale<S> {
    type Strategy = proptest::strategy::BoxedStrategy<Decimal>;

    #[inline]
    fn arbitrary_decimal() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let () = Self::VALID;
        // Construct directly with the required scale — no filtering.
        // Constrain mantissa to the Decimal-representable range so
        // `from_i128_with_scale` cannot panic.
        (DECIMAL_MIN_MANTISSA..=DECIMAL_MAX_MANTISSA)
            .prop_map(|m| Decimal::from_i128_with_scale(m, u32::from(S)))
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const P: u8> ArbitraryDecimal for DecimalPrecision<P> {
    type Strategy = proptest::strategy::BoxedStrategy<Decimal>;

    #[inline]
    fn arbitrary_decimal() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        // Construct mantissas inside the precision bound directly so
        // sparse precisions (e.g. P=5 over Decimal's 96-bit range)
        // do not rely on rejection sampling. For P >= 29 the bound
        // saturates at the full Decimal mantissa range; for P == 0
        // only zero is admissible. The bound is resolved in a `const`
        // block so the per-`P` selection happens at compile time —
        // no runtime branch to leave one direction structurally dead
        // in each monomorphisation.
        let max_mantissa: i128 = const {
            if P == 0 {
                0
            } else if P >= 29 {
                DECIMAL_MAX_MANTISSA
            } else {
                10_i128.pow(P as u32) - 1
            }
        };
        let min_mantissa = -max_mantissa;
        (
            min_mantissa..=max_mantissa,
            0_u32..=u32::from(DECIMAL_MAX_SCALE_U8),
        )
            .prop_map(|(m, s)| Decimal::from_i128_with_scale(m, s))
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MIN_REPR: i128, const MAX_REPR: i128, const SCALE: u8> ArbitraryDecimal
    for DecimalInRange<MIN_REPR, MAX_REPR, SCALE>
{
    type Strategy = proptest::strategy::BoxedStrategy<Decimal>;

    #[inline]
    fn arbitrary_decimal() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let () = Self::VALID;
        // Sample the mantissa directly inside the admissible
        // integer range — no rejection sampling.
        (MIN_REPR..=MAX_REPR)
            .prop_map(|m| Decimal::from_i128_with_scale(m, u32::from(SCALE)))
            .boxed()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "explicit in test code"
)]
mod tests {
    use alloc::string::ToString;

    use rust_decimal::Decimal;

    use crate::Refined;

    use super::{
        DecimalError, DecimalInRange, DecimalPositive, DecimalPrecision, DecimalScale,
        significant_digits,
    };

    // ─── DecimalPositive. ─────────────────────────────────────────

    #[test]
    fn decimal_positive_admits_strictly_positive() {
        let r: Refined<Decimal, DecimalPositive> = Refined::try_new(Decimal::new(123, 2)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(123, 2));
    }

    #[test]
    fn decimal_positive_rejects_zero() {
        let bad: Result<Refined<Decimal, DecimalPositive>, _> = Refined::try_new(Decimal::ZERO);
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::NotPositive {
                value: Decimal::ZERO,
            },
        );
    }

    #[test]
    fn decimal_positive_rejects_negative() {
        let bad: Result<Refined<Decimal, DecimalPositive>, _> =
            Refined::try_new(Decimal::new(-1, 0));
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::NotPositive {
                value: Decimal::new(-1, 0),
            },
        );
    }

    // ─── DecimalScale. ────────────────────────────────────────────

    #[test]
    fn decimal_scale_admits_matching_scale() {
        let r: Refined<Decimal, DecimalScale<2>> = Refined::try_new(Decimal::new(999, 2)).unwrap();
        assert_eq!(r.as_inner().scale(), 2);
    }

    #[test]
    fn decimal_scale_rejects_smaller_scale() {
        let bad: Result<Refined<Decimal, DecimalScale<2>>, _> =
            Refined::try_new(Decimal::new(10, 0));
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::ScaleMismatch {
                expected: 2,
                actual: 0,
            },
        );
    }

    #[test]
    fn decimal_scale_rejects_larger_scale() {
        let bad: Result<Refined<Decimal, DecimalScale<2>>, _> =
            Refined::try_new(Decimal::new(1234, 3));
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::ScaleMismatch {
                expected: 2,
                actual: 3,
            },
        );
    }

    // ─── DecimalPrecision. ────────────────────────────────────────

    #[test]
    fn significant_digits_handles_zero() {
        assert_eq!(significant_digits(0), 0);
    }

    #[test]
    fn significant_digits_handles_positive_and_negative() {
        assert_eq!(significant_digits(1), 1);
        assert_eq!(significant_digits(9), 1);
        assert_eq!(significant_digits(10), 2);
        assert_eq!(significant_digits(99_999), 5);
        assert_eq!(significant_digits(-99_999), 5);
        assert_eq!(significant_digits(i128::MIN), 39);
    }

    #[test]
    fn decimal_precision_admits_zero_for_any_p() {
        let r: Refined<Decimal, DecimalPrecision<0>> = Refined::try_new(Decimal::ZERO).unwrap();
        assert_eq!(*r.as_inner(), Decimal::ZERO);
    }

    #[test]
    fn decimal_precision_admits_within_limit() {
        let r: Refined<Decimal, DecimalPrecision<5>> =
            Refined::try_new(Decimal::new(12_345, 0)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(12_345, 0));
    }

    #[test]
    fn decimal_precision_rejects_above_limit() {
        let bad: Result<Refined<Decimal, DecimalPrecision<5>>, _> =
            Refined::try_new(Decimal::new(123_456, 0));
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::PrecisionExceeded {
                limit: 5,
                actual: 6,
            },
        );
    }

    // ─── DecimalInRange. ──────────────────────────────────────────

    #[test]
    fn decimal_in_range_admits_value_within_bounds() {
        type Pct = DecimalInRange<0, 10_000, 2>;
        let r: Refined<Decimal, Pct> = Refined::try_new(Decimal::new(5_000, 2)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(5_000, 2));
    }

    #[test]
    fn decimal_in_range_admits_lower_endpoint() {
        type Pct = DecimalInRange<0, 10_000, 2>;
        let r: Refined<Decimal, Pct> = Refined::try_new(Decimal::new(0, 2)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(0, 2));
    }

    #[test]
    fn decimal_in_range_admits_upper_endpoint() {
        type Pct = DecimalInRange<0, 10_000, 2>;
        let r: Refined<Decimal, Pct> = Refined::try_new(Decimal::new(10_000, 2)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(10_000, 2));
    }

    #[test]
    fn decimal_in_range_rejects_above_upper() {
        type Pct = DecimalInRange<0, 10_000, 2>;
        let bad_value = Decimal::new(15_000, 2);
        let bad: Result<Refined<Decimal, Pct>, _> = Refined::try_new(bad_value);
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::OutOfRange { value: bad_value }
        );
    }

    #[test]
    fn decimal_in_range_rejects_below_lower() {
        type Pct = DecimalInRange<0, 10_000, 2>;
        let bad_value = Decimal::new(-1, 2);
        let bad: Result<Refined<Decimal, Pct>, _> = Refined::try_new(bad_value);
        assert_eq!(
            bad.unwrap_err(),
            DecimalError::OutOfRange { value: bad_value }
        );
    }

    #[test]
    fn decimal_in_range_negative_range() {
        type NegPct = DecimalInRange<-10_000, 0, 2>;
        let r: Refined<Decimal, NegPct> = Refined::try_new(Decimal::new(-5_000, 2)).unwrap();
        assert_eq!(*r.as_inner(), Decimal::new(-5_000, 2));
    }

    // ─── Display. ─────────────────────────────────────────────────

    #[test]
    fn display_not_positive() {
        let err = DecimalError::NotPositive {
            value: Decimal::new(-1, 0),
        };
        assert_eq!(err.to_string(), "value -1 is not strictly positive");
    }

    #[test]
    fn display_scale_mismatch() {
        let err = DecimalError::ScaleMismatch {
            expected: 2,
            actual: 3,
        };
        assert_eq!(
            err.to_string(),
            "value scale 3 does not equal required scale 2",
        );
    }

    #[test]
    fn display_precision_exceeded() {
        let err = DecimalError::PrecisionExceeded {
            limit: 5,
            actual: 6,
        };
        assert_eq!(
            err.to_string(),
            "value has 6 significant digits; limit is 5",
        );
    }

    #[test]
    fn display_out_of_range() {
        let err = DecimalError::OutOfRange {
            value: Decimal::new(1234, 2),
        };
        assert_eq!(err.to_string(), "value 12.34 out of admissible range");
    }

    // ─── Arbitrary strategy soundness. ────────────────────────────
    //
    // Each rule's `ArbitraryDecimal` strategy MUST emit only values
    // that pass `refine`. The blanket `Refined<T, R>: Arbitrary`
    // impl `expect`s on `try_new`, so a strategy bug surfaces as a
    // proptest panic. These tests check the property directly.

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_decimal_positive_value_is_positive(
            r in proptest::arbitrary::any::<Refined<Decimal, DecimalPositive>>()
        ) {
            proptest::prop_assert!(*r.as_inner() > Decimal::ZERO);
        }

        #[test]
        fn arbitrary_decimal_scale_value_has_required_scale(
            r in proptest::arbitrary::any::<Refined<Decimal, DecimalScale<2>>>()
        ) {
            proptest::prop_assert_eq!(r.as_inner().scale(), 2);
        }

        #[test]
        fn arbitrary_decimal_precision_value_within_limit(
            r in proptest::arbitrary::any::<Refined<Decimal, DecimalPrecision<5>>>()
        ) {
            proptest::prop_assert!(
                super::significant_digits(r.as_inner().mantissa()) <= 5
            );
        }

        #[test]
        fn arbitrary_decimal_precision_zero_admits_only_zero(
            r in proptest::arbitrary::any::<Refined<Decimal, DecimalPrecision<0>>>()
        ) {
            // `P == 0` admits zero significant digits, so the
            // strategy's mantissa bound collapses to `0`.
            proptest::prop_assert_eq!(r.as_inner().mantissa(), 0);
        }

        #[test]
        fn arbitrary_decimal_precision_saturated_within_limit(
            r in proptest::arbitrary::any::<Refined<Decimal, DecimalPrecision<29>>>()
        ) {
            // `P >= 29` saturates the bound at the full Decimal
            // mantissa range; every emitted value stays within the
            // 29-significant-digit limit by construction.
            proptest::prop_assert!(
                super::significant_digits(r.as_inner().mantissa()) <= 29
            );
        }

        #[test]
        fn arbitrary_decimal_in_range_value_in_bounds(
            r in proptest::arbitrary::any::<
                Refined<Decimal, DecimalInRange<0, 10_000, 2>>,
            >()
        ) {
            let lo = Decimal::new(0, 2);
            let hi = Decimal::new(10_000, 2);
            let v = *r.as_inner();
            proptest::prop_assert!(v >= lo && v <= hi);
        }
    }
}
