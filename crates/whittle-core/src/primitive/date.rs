//! Date primitive rules.
//!
//! Validation rules for `chrono::NaiveDate`. Each primitive is a
//! `Rule<NaiveDate>` and returns the flat [`DateError`] enum.
//!
//! Available behind the `chrono` Cargo feature.
//!
//! Range bounds are encoded as `i32` days from CE (year 1, day 1) —
//! the value returned by `NaiveDate::num_days_from_ce` — because Rust
//! 2024 does not yet allow `NaiveDate` const generics:
//!
//! - `NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().num_days_from_ce()`
//!   is `730_120`.
//! - `NaiveDate::from_ymd_opt(2100, 12, 31).unwrap().num_days_from_ce()`
//!   is `767_009`.
//! - `NaiveDate::from_ymd_opt(2024, 5, 28).unwrap().num_days_from_ce()`
//!   is `739_034`.
//!
//! Compute the bound once and write it as a const-generic literal.
//! `NaiveDate` represents dates in roughly `[-262_143-01-01,
//! +262_142-12-31]`; values outside that range are caught at compile
//! time via `const { NaiveDate::from_num_days_from_ce_opt(...) }`.

use chrono::{Datelike, NaiveDate};

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;

/// Flat domain error for date rules.
#[derive(Debug, PartialEq, Eq)]
pub enum DateError {
    /// Value is outside the admissible closed range. Returned by
    /// [`DateAtLeast`], [`DateAtMost`], and [`DateInRange`]. Carries
    /// the offending value.
    OutOfRange {
        /// Offending value.
        value: NaiveDate,
    },
}

impl core::fmt::Display for DateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::OutOfRange { value } => {
                write!(f, "date {value} out of admissible range")
            }
        }
    }
}

impl core::error::Error for DateError {}

/// Compile-time conversion of an `i32` days-from-CE bound into a
/// `NaiveDate`. Fails at const-eval if the value is out of
/// `NaiveDate`'s representable range.
#[inline]
const fn date_from_ce_days(days_from_ce: i32) -> NaiveDate {
    NaiveDate::from_num_days_from_ce_opt(days_from_ce)
        .expect("date bound out of chrono::NaiveDate range")
}

/// Reject dates strictly before `MIN_DAYS_FROM_CE` (as
/// `NaiveDate::num_days_from_ce`).
///
/// `MIN_DAYS_FROM_CE` must lie within `NaiveDate`'s representable
/// range — enforced at compile time via `const { ... }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{Datelike, NaiveDate};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateAtLeast, DateError};
///
/// // 2000-01-01 = 730_120 days from CE.
/// type SinceY2K = DateAtLeast<730_120>;
///
/// // Admit: a date after the bound.
/// let ok: Refined<NaiveDate, SinceY2K> =
///     Refined::try_new(NaiveDate::from_ymd_opt(2024, 5, 28).unwrap()).unwrap();
/// assert_eq!(ok.as_inner().num_days_from_ce(), 739_034);
///
/// // Reject: a date before the bound.
/// let bad = NaiveDate::from_ymd_opt(1999, 12, 31).unwrap();
/// let err = Refined::<NaiveDate, SinceY2K>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateAtLeast<const MIN_DAYS_FROM_CE: i32>;

impl<const MIN_DAYS_FROM_CE: i32> DateAtLeast<MIN_DAYS_FROM_CE> {
    const MIN_DATE: NaiveDate = date_from_ce_days(MIN_DAYS_FROM_CE);
}

impl<const MIN_DAYS_FROM_CE: i32> Rule<NaiveDate> for DateAtLeast<MIN_DAYS_FROM_CE> {
    type Error = DateError;

    #[inline]
    fn refine(raw: NaiveDate) -> Result<NaiveDate, Self::Error> {
        if raw >= Self::MIN_DATE {
            Ok(raw)
        } else {
            Err(DateError::OutOfRange { value: raw })
        }
    }
}

/// Reject dates strictly after `MAX_DAYS_FROM_CE`.
///
/// `MAX_DAYS_FROM_CE` must lie within `NaiveDate`'s representable
/// range — enforced at compile time via `const { ... }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{Datelike, NaiveDate};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateAtMost, DateError};
///
/// // 2100-12-31 = 767_009 days from CE.
/// type ThisCentury = DateAtMost<767_009>;
///
/// // Admit: a date at or before the bound.
/// let ok: Refined<NaiveDate, ThisCentury> =
///     Refined::try_new(NaiveDate::from_ymd_opt(2100, 12, 31).unwrap()).unwrap();
/// assert_eq!(ok.as_inner().num_days_from_ce(), 767_009);
///
/// // Reject: a date after the bound.
/// let bad = NaiveDate::from_ymd_opt(2101, 1, 1).unwrap();
/// let err = Refined::<NaiveDate, ThisCentury>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateAtMost<const MAX_DAYS_FROM_CE: i32>;

impl<const MAX_DAYS_FROM_CE: i32> DateAtMost<MAX_DAYS_FROM_CE> {
    const MAX_DATE: NaiveDate = date_from_ce_days(MAX_DAYS_FROM_CE);
}

impl<const MAX_DAYS_FROM_CE: i32> Rule<NaiveDate> for DateAtMost<MAX_DAYS_FROM_CE> {
    type Error = DateError;

    #[inline]
    fn refine(raw: NaiveDate) -> Result<NaiveDate, Self::Error> {
        if raw <= Self::MAX_DATE {
            Ok(raw)
        } else {
            Err(DateError::OutOfRange { value: raw })
        }
    }
}

/// Closed range `[MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE]` in CE days.
///
/// Nominal domain rule that hides
/// `And<DateAtLeast<MIN_DAYS_FROM_CE>, DateAtMost<MAX_DAYS_FROM_CE>>`
/// and surfaces the shared `DateError` directly. Both endpoints are
/// admissible. Both bounds must lie within `NaiveDate`'s range, and
/// `MIN_DAYS_FROM_CE <= MAX_DAYS_FROM_CE` — both checks happen at
/// compile time.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{Datelike, NaiveDate};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateError, DateInRange};
///
/// // 2000-01-01 ..= 2100-12-31.
/// type ThisMillennium = DateInRange<730_120, 767_009>;
///
/// // Admit: midpoint.
/// let ok: Refined<NaiveDate, ThisMillennium> =
///     Refined::try_new(NaiveDate::from_ymd_opt(2050, 6, 15).unwrap()).unwrap();
/// assert_eq!(ok.as_inner().year(), 2050);
///
/// // Reject: before the lower bound.
/// let bad = NaiveDate::from_ymd_opt(1999, 12, 31).unwrap();
/// let err = Refined::<NaiveDate, ThisMillennium>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateInRange<const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32>;

impl<const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32>
    DateInRange<MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE>
{
    const VALID: () = const {
        // Force compile-time validation of both endpoints against
        // `NaiveDate`'s representable range.
        let _lo: NaiveDate = date_from_ce_days(MIN_DAYS_FROM_CE);
        let _hi: NaiveDate = date_from_ce_days(MAX_DAYS_FROM_CE);
        assert!(
            MIN_DAYS_FROM_CE <= MAX_DAYS_FROM_CE,
            "DateInRange<MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE>: MIN_DAYS_FROM_CE must be <= MAX_DAYS_FROM_CE",
        );
    };
}

impl<const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32> Rule<NaiveDate>
    for DateInRange<MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE>
{
    type Error = DateError;

    #[inline]
    fn refine(raw: NaiveDate) -> Result<NaiveDate, Self::Error> {
        let () = Self::VALID;
        <crate::composition::And<
            DateAtLeast<MIN_DAYS_FROM_CE>,
            DateAtMost<MAX_DAYS_FROM_CE>,
        > as Rule<NaiveDate>>::refine(raw)
    }
}

/// Per-rule date strategy used by the blanket
/// `Refined<NaiveDate, R>: Arbitrary` impl.
///
/// Each rule samples directly inside its admissible interval — no
/// rejection sampling. The blanket `Refined<NaiveDate, R>:
/// Arbitrary` impl maps the strategy through `Refined::try_new`.
///
/// Available behind the `chrono` and `proptest` features.
#[cfg(feature = "proptest")]
pub trait ArbitraryDate: Rule<NaiveDate> {
    /// Concrete strategy type. Always resolves to a
    /// [`proptest::strategy::BoxedStrategy`] for API stability.
    type Strategy: proptest::strategy::Strategy<Value = NaiveDate>;

    /// Build the rule's `NaiveDate` strategy.
    fn arbitrary_date() -> Self::Strategy;
}

// ─── Serde `DeserializeRule` impls: default parse-then-refine.
//      Applicable only when `chrono`'s own `serde` support is
//      enabled by the consumer (the `NaiveDate: Deserialize<'de>`
//      bound is satisfied through feature unification). ────────────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MIN_DAYS_FROM_CE: i32] DeserializeRule<NaiveDate>
    for DateAtLeast<MIN_DAYS_FROM_CE>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MAX_DAYS_FROM_CE: i32] DeserializeRule<NaiveDate>
    for DateAtMost<MAX_DAYS_FROM_CE>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32] DeserializeRule<NaiveDate>
    for DateInRange<MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE>
}

#[cfg(feature = "proptest")]
impl<R: ArbitraryDate> ArbitraryRule<NaiveDate> for R {
    type Strategy = R::Strategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        R::arbitrary_date()
    }
}

#[cfg(feature = "proptest")]
fn naive_date_min_days_from_ce() -> i32 {
    NaiveDate::MIN.num_days_from_ce()
}

#[cfg(feature = "proptest")]
fn naive_date_max_days_from_ce() -> i32 {
    NaiveDate::MAX.num_days_from_ce()
}

#[cfg(feature = "proptest")]
impl<const MIN_DAYS_FROM_CE: i32> ArbitraryDate for DateAtLeast<MIN_DAYS_FROM_CE> {
    type Strategy = proptest::strategy::BoxedStrategy<NaiveDate>;

    #[inline]
    fn arbitrary_date() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        (MIN_DAYS_FROM_CE..=naive_date_max_days_from_ce())
            .prop_map(date_from_ce_days)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MAX_DAYS_FROM_CE: i32> ArbitraryDate for DateAtMost<MAX_DAYS_FROM_CE> {
    type Strategy = proptest::strategy::BoxedStrategy<NaiveDate>;

    #[inline]
    fn arbitrary_date() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        (naive_date_min_days_from_ce()..=MAX_DAYS_FROM_CE)
            .prop_map(date_from_ce_days)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MIN_DAYS_FROM_CE: i32, const MAX_DAYS_FROM_CE: i32> ArbitraryDate
    for DateInRange<MIN_DAYS_FROM_CE, MAX_DAYS_FROM_CE>
{
    type Strategy = proptest::strategy::BoxedStrategy<NaiveDate>;

    #[inline]
    fn arbitrary_date() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let () = Self::VALID;
        (MIN_DAYS_FROM_CE..=MAX_DAYS_FROM_CE)
            .prop_map(date_from_ce_days)
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

    use chrono::{Datelike, NaiveDate};

    use crate::Refined;

    use super::{DateAtLeast, DateAtMost, DateError, DateInRange};

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    // ─── DateAtLeast. ─────────────────────────────────────────────

    #[test]
    fn date_at_least_admits_endpoint() {
        type SinceY2K = DateAtLeast<730_120>;
        let r: Refined<NaiveDate, SinceY2K> = Refined::try_new(ymd(2000, 1, 1)).unwrap();
        assert_eq!(*r.as_inner(), ymd(2000, 1, 1));
    }

    #[test]
    fn date_at_least_admits_above() {
        type SinceY2K = DateAtLeast<730_120>;
        let r: Refined<NaiveDate, SinceY2K> = Refined::try_new(ymd(2024, 5, 28)).unwrap();
        assert_eq!(r.as_inner().year(), 2024);
    }

    #[test]
    fn date_at_least_rejects_below() {
        type SinceY2K = DateAtLeast<730_120>;
        let bad = ymd(1999, 12, 31);
        let res: Result<Refined<NaiveDate, SinceY2K>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateError::OutOfRange { value: bad });
    }

    // ─── DateAtMost. ──────────────────────────────────────────────

    #[test]
    fn date_at_most_admits_endpoint() {
        type ThisCentury = DateAtMost<767_009>;
        let r: Refined<NaiveDate, ThisCentury> = Refined::try_new(ymd(2100, 12, 31)).unwrap();
        assert_eq!(*r.as_inner(), ymd(2100, 12, 31));
    }

    #[test]
    fn date_at_most_admits_below() {
        type ThisCentury = DateAtMost<767_009>;
        let r: Refined<NaiveDate, ThisCentury> = Refined::try_new(ymd(2024, 5, 28)).unwrap();
        assert_eq!(r.as_inner().year(), 2024);
    }

    #[test]
    fn date_at_most_rejects_above() {
        type ThisCentury = DateAtMost<767_009>;
        let bad = ymd(2101, 1, 1);
        let res: Result<Refined<NaiveDate, ThisCentury>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateError::OutOfRange { value: bad });
    }

    // ─── DateInRange. ─────────────────────────────────────────────

    #[test]
    fn date_in_range_admits_midpoint() {
        type ThisMillennium = DateInRange<730_120, 767_009>;
        let r: Refined<NaiveDate, ThisMillennium> = Refined::try_new(ymd(2050, 6, 15)).unwrap();
        assert_eq!(r.as_inner().year(), 2050);
    }

    #[test]
    fn date_in_range_admits_lower_endpoint() {
        type ThisMillennium = DateInRange<730_120, 767_009>;
        let r: Refined<NaiveDate, ThisMillennium> = Refined::try_new(ymd(2000, 1, 1)).unwrap();
        assert_eq!(*r.as_inner(), ymd(2000, 1, 1));
    }

    #[test]
    fn date_in_range_admits_upper_endpoint() {
        type ThisMillennium = DateInRange<730_120, 767_009>;
        let r: Refined<NaiveDate, ThisMillennium> = Refined::try_new(ymd(2100, 12, 31)).unwrap();
        assert_eq!(*r.as_inner(), ymd(2100, 12, 31));
    }

    #[test]
    fn date_in_range_rejects_below_lower() {
        type ThisMillennium = DateInRange<730_120, 767_009>;
        let bad = ymd(1999, 12, 31);
        let res: Result<Refined<NaiveDate, ThisMillennium>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateError::OutOfRange { value: bad });
    }

    #[test]
    fn date_in_range_rejects_above_upper() {
        type ThisMillennium = DateInRange<730_120, 767_009>;
        let bad = ymd(2101, 1, 1);
        let res: Result<Refined<NaiveDate, ThisMillennium>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateError::OutOfRange { value: bad });
    }

    // ─── Display. ─────────────────────────────────────────────────

    #[test]
    fn display_out_of_range() {
        let err = DateError::OutOfRange {
            value: ymd(1999, 12, 31),
        };
        assert_eq!(err.to_string(), "date 1999-12-31 out of admissible range");
    }

    // ─── Arbitrary strategy soundness. ────────────────────────────

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_date_at_least_value_in_range(
            r in proptest::arbitrary::any::<Refined<NaiveDate, DateAtLeast<730_120>>>()
        ) {
            proptest::prop_assert!(r.as_inner().num_days_from_ce() >= 730_120);
        }

        #[test]
        fn arbitrary_date_at_most_value_in_range(
            r in proptest::arbitrary::any::<Refined<NaiveDate, DateAtMost<767_009>>>()
        ) {
            proptest::prop_assert!(r.as_inner().num_days_from_ce() <= 767_009);
        }

        #[test]
        fn arbitrary_date_in_range_value_in_range(
            r in proptest::arbitrary::any::<
                Refined<NaiveDate, DateInRange<730_120, 767_009>>,
            >()
        ) {
            let days = r.as_inner().num_days_from_ce();
            proptest::prop_assert!((730_120..=767_009).contains(&days));
        }
    }
}
