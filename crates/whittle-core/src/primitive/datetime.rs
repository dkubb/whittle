//! `DateTime<Utc>` primitive rules.
//!
//! Validation rules for `chrono::DateTime<chrono::Utc>`. Each
//! primitive is a `Rule<DateTime<Utc>>` and returns the flat
//! [`DateTimeError`] enum.
//!
//! Available behind the `chrono` Cargo feature.
//!
//! Range bounds are encoded as `i64` seconds since the Unix epoch
//! (the value returned by `DateTime::<Utc>::timestamp`) because
//! Rust 2024 does not yet allow `DateTime` const generics:
//!
//! - `DateTime::<Utc>::from_timestamp(0, 0).unwrap()` is
//!   `1970-01-01 00:00:00 UTC` — timestamp `0`.
//! - `2024-01-01 00:00:00 UTC` has timestamp `1_704_067_200`.
//! - `2030-01-01 00:00:00 UTC` has timestamp `1_893_456_000`.
//!
//! Compute the bound once and write it as a const-generic literal.
//! `DateTime<Utc>` spans roughly the same `NaiveDateTime` range as
//! `NaiveDate`; values outside it are caught at compile time via
//! `const { DateTime::<Utc>::from_timestamp(...) }`.
//!
//! This module supports only `DateTime<Utc>` — wall-clock-zoned
//! datetimes (`DateTime<FixedOffset>`, `DateTime<Local>`) are not
//! exposed; convert to UTC at the boundary.

use chrono::{DateTime, Utc};

#[cfg(feature = "proptest")]
use crate::rule::ArbitraryRule;
use crate::rule::Rule;
use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

/// Flat domain error for `DateTime<Utc>` rules.
#[derive(Debug, PartialEq, Eq)]
pub enum DateTimeError {
    /// Value is outside the admissible closed range. Returned by
    /// [`DateTimeAtLeast`], [`DateTimeAtMost`], and
    /// [`DateTimeInRange`]. Carries the offending value.
    OutOfRange {
        /// Offending value.
        value: DateTime<Utc>,
    },
}

impl core::fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::OutOfRange { value } => {
                write!(f, "datetime {value} out of admissible range")
            }
        }
    }
}

impl core::error::Error for DateTimeError {}

/// Compile-time conversion of an `i64` seconds-since-Unix-epoch
/// bound into a `DateTime<Utc>`. Fails at const-eval if the value
/// is out of `DateTime<Utc>`'s representable range.
#[inline]
const fn datetime_from_epoch_secs(secs_since_epoch: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(secs_since_epoch, 0)
        .expect("datetime bound out of chrono::DateTime<Utc> range")
}

/// Reject datetimes strictly before `MIN_SECS_SINCE_EPOCH` (as
/// `DateTime::<Utc>::timestamp`).
///
/// `MIN_SECS_SINCE_EPOCH` must lie within `DateTime<Utc>`'s
/// representable range — enforced at compile time via `const { ... }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{DateTime, TimeZone, Utc};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateTimeAtLeast, DateTimeError};
///
/// // 2024-01-01 00:00:00 UTC = 1_704_067_200 seconds since epoch.
/// type Since2024 = DateTimeAtLeast<1_704_067_200>;
///
/// // Admit: a datetime after the bound.
/// let later = Utc.with_ymd_and_hms(2024, 5, 28, 12, 0, 0).unwrap();
/// let ok: Refined<DateTime<Utc>, Since2024> =
///     Refined::try_new(later).unwrap();
/// assert_eq!(*ok.as_inner(), later);
///
/// // Reject: a datetime before the bound.
/// let bad = Utc.with_ymd_and_hms(2023, 12, 31, 23, 59, 59).unwrap();
/// let err = Refined::<DateTime<Utc>, Since2024>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateTimeError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateTimeAtLeast<const MIN_SECS_SINCE_EPOCH: i64>;

impl<const MIN_SECS_SINCE_EPOCH: i64> DateTimeAtLeast<MIN_SECS_SINCE_EPOCH> {
    const MIN_DATETIME: DateTime<Utc> = datetime_from_epoch_secs(MIN_SECS_SINCE_EPOCH);
}

impl<const MIN_SECS_SINCE_EPOCH: i64> Rule<DateTime<Utc>>
    for DateTimeAtLeast<MIN_SECS_SINCE_EPOCH>
{
    type Error = DateTimeError;

    #[inline]
    fn refine(raw: DateTime<Utc>) -> Result<DateTime<Utc>, Self::Error> {
        if raw >= Self::MIN_DATETIME {
            Ok(raw)
        } else {
            Err(DateTimeError::OutOfRange { value: raw })
        }
    }
}

/// Reject datetimes strictly after `MAX_SECS_SINCE_EPOCH`.
///
/// `MAX_SECS_SINCE_EPOCH` must lie within `DateTime<Utc>`'s
/// representable range — enforced at compile time via `const { ... }`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{DateTime, TimeZone, Utc};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateTimeAtMost, DateTimeError};
///
/// // 2030-01-01 00:00:00 UTC = 1_893_456_000 seconds since epoch.
/// type Until2030 = DateTimeAtMost<1_893_456_000>;
///
/// // Admit: a datetime at or before the bound.
/// let inside = Utc.with_ymd_and_hms(2024, 5, 28, 12, 0, 0).unwrap();
/// let ok: Refined<DateTime<Utc>, Until2030> =
///     Refined::try_new(inside).unwrap();
/// assert_eq!(*ok.as_inner(), inside);
///
/// // Reject: a datetime after the bound.
/// let bad = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 1).unwrap();
/// let err = Refined::<DateTime<Utc>, Until2030>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateTimeError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateTimeAtMost<const MAX_SECS_SINCE_EPOCH: i64>;

impl<const MAX_SECS_SINCE_EPOCH: i64> DateTimeAtMost<MAX_SECS_SINCE_EPOCH> {
    const MAX_DATETIME: DateTime<Utc> = datetime_from_epoch_secs(MAX_SECS_SINCE_EPOCH);
}

impl<const MAX_SECS_SINCE_EPOCH: i64> Rule<DateTime<Utc>> for DateTimeAtMost<MAX_SECS_SINCE_EPOCH> {
    type Error = DateTimeError;

    #[inline]
    fn refine(raw: DateTime<Utc>) -> Result<DateTime<Utc>, Self::Error> {
        if raw <= Self::MAX_DATETIME {
            Ok(raw)
        } else {
            Err(DateTimeError::OutOfRange { value: raw })
        }
    }
}

/// Closed range `[MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH]` in
/// seconds since the Unix epoch.
///
/// Nominal domain rule that hides `And<DateTimeAtLeast<MIN>,
/// DateTimeAtMost<MAX>>` and surfaces the shared `DateTimeError`
/// directly. Both endpoints are admissible. Both bounds must lie
/// within `DateTime<Utc>`'s range, and `MIN <= MAX` — both checks
/// happen at compile time.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use chrono::{DateTime, TimeZone, Utc};
/// use whittle_core::Refined;
/// use whittle_core::primitive::{DateTimeError, DateTimeInRange};
///
/// // 2024-01-01 ..= 2030-01-01 UTC.
/// type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
///
/// // Admit: midpoint.
/// let mid = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
/// let ok: Refined<DateTime<Utc>, ThisDecade> =
///     Refined::try_new(mid).unwrap();
/// assert_eq!(*ok.as_inner(), mid);
///
/// // Reject: before the lower bound.
/// let bad = Utc.with_ymd_and_hms(2023, 12, 31, 23, 59, 59).unwrap();
/// let err = Refined::<DateTime<Utc>, ThisDecade>::try_new(bad).unwrap_err();
/// assert_eq!(err, DateTimeError::OutOfRange { value: bad });
/// # }
/// ```
pub struct DateTimeInRange<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64>;

impl<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64>
    DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>
{
    const VALID: () = const {
        // Force compile-time validation of both endpoints against
        // `DateTime<Utc>`'s representable range.
        let _lo: DateTime<Utc> = datetime_from_epoch_secs(MIN_SECS_SINCE_EPOCH);
        let _hi: DateTime<Utc> = datetime_from_epoch_secs(MAX_SECS_SINCE_EPOCH);
        assert!(
            MIN_SECS_SINCE_EPOCH <= MAX_SECS_SINCE_EPOCH,
            "DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>: MIN_SECS_SINCE_EPOCH must be <= MAX_SECS_SINCE_EPOCH",
        );
    };
}

impl<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64> Rule<DateTime<Utc>>
    for DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>
{
    type Error = DateTimeError;

    #[inline]
    fn refine(raw: DateTime<Utc>) -> Result<DateTime<Utc>, Self::Error> {
        let () = Self::VALID;
        <crate::composition::And<
            DateTimeAtLeast<MIN_SECS_SINCE_EPOCH>,
            DateTimeAtMost<MAX_SECS_SINCE_EPOCH>,
        > as Rule<DateTime<Utc>>>::refine(raw)
    }
}

/// Per-rule `DateTime<Utc>` strategy used by the blanket
/// `Refined<DateTime<Utc>, R>: Arbitrary` impl.
///
/// Each rule samples directly inside its admissible interval — no
/// rejection sampling. The blanket `Refined<DateTime<Utc>, R>:
/// Arbitrary` impl maps the strategy through `Refined::try_new`.
///
/// Available behind the `chrono` and `proptest` features.
#[cfg(feature = "proptest")]
pub trait ArbitraryDateTime: Rule<DateTime<Utc>> {
    /// Concrete strategy type. Always resolves to a
    /// [`proptest::strategy::BoxedStrategy`] for API stability.
    type Strategy: proptest::strategy::Strategy<Value = DateTime<Utc>>;

    /// Build the rule's `DateTime<Utc>` strategy.
    fn arbitrary_datetime() -> Self::Strategy;
}

// ─── `SchemaRule` impls. ──────────────────────────────────────────
//
// Datetime schemas are integer intervals of kind `DateTime` whose
// endpoints are seconds since the Unix epoch — read back from the
// SAME bound consts `refine` compares against (`MIN_DATETIME` /
// `MAX_DATETIME`), so the compile-time range validation is forced
// here exactly as it is in `refine`.

impl<const MIN_SECS_SINCE_EPOCH: i64> SchemaRule<DateTime<Utc>>
    for DateTimeAtLeast<MIN_SECS_SINCE_EPOCH>
{
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::DateTime,
            Bound::Inclusive(Scalar::Int(i128::from(Self::MIN_DATETIME.timestamp()))),
            Bound::Unbounded,
        )
    }
}

impl<const MAX_SECS_SINCE_EPOCH: i64> SchemaRule<DateTime<Utc>>
    for DateTimeAtMost<MAX_SECS_SINCE_EPOCH>
{
    #[inline]
    fn schema() -> Schema {
        Schema::interval(
            ScalarKind::DateTime,
            Bound::Unbounded,
            Bound::Inclusive(Scalar::Int(i128::from(Self::MAX_DATETIME.timestamp()))),
        )
    }
}

impl<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64> SchemaRule<DateTime<Utc>>
    for DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>
{
    #[inline]
    fn schema() -> Schema {
        let () = Self::VALID;
        Schema::interval(
            ScalarKind::DateTime,
            Bound::Inclusive(Scalar::Int(i128::from(MIN_SECS_SINCE_EPOCH))),
            Bound::Inclusive(Scalar::Int(i128::from(MAX_SECS_SINCE_EPOCH))),
        )
    }
}

// ─── Serde `DeserializeRule` impls: default parse-then-refine.
//      Applicable only when `chrono`'s own `serde` support is
//      enabled by the consumer (the `DateTime<Utc>:
//      Deserialize<'de>` bound is satisfied through feature
//      unification). ──────────────────────────────────────────────

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MIN_SECS_SINCE_EPOCH: i64] DeserializeRule<DateTime<Utc>>
    for DateTimeAtLeast<MIN_SECS_SINCE_EPOCH>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MAX_SECS_SINCE_EPOCH: i64] DeserializeRule<DateTime<Utc>>
    for DateTimeAtMost<MAX_SECS_SINCE_EPOCH>
}

#[cfg(feature = "serde")]
crate::deserialize_rule! {
    impl[const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64]
    DeserializeRule<DateTime<Utc>>
    for DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>
}

#[cfg(feature = "proptest")]
impl<R: ArbitraryDateTime> ArbitraryRule<DateTime<Utc>> for R {
    type Strategy = R::Strategy;

    #[inline]
    fn arbitrary_strategy() -> Self::Strategy {
        R::arbitrary_datetime()
    }
}

#[cfg(feature = "proptest")]
const DATETIME_MIN_SECS: i64 = DateTime::<Utc>::MIN_UTC.timestamp();
#[cfg(feature = "proptest")]
const DATETIME_MAX_SECS: i64 = DateTime::<Utc>::MAX_UTC.timestamp();

#[cfg(feature = "proptest")]
impl<const MIN_SECS_SINCE_EPOCH: i64> ArbitraryDateTime for DateTimeAtLeast<MIN_SECS_SINCE_EPOCH> {
    type Strategy = proptest::strategy::BoxedStrategy<DateTime<Utc>>;

    #[inline]
    fn arbitrary_datetime() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        (MIN_SECS_SINCE_EPOCH..=DATETIME_MAX_SECS)
            .prop_map(datetime_from_epoch_secs)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MAX_SECS_SINCE_EPOCH: i64> ArbitraryDateTime for DateTimeAtMost<MAX_SECS_SINCE_EPOCH> {
    type Strategy = proptest::strategy::BoxedStrategy<DateTime<Utc>>;

    #[inline]
    fn arbitrary_datetime() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        (DATETIME_MIN_SECS..=MAX_SECS_SINCE_EPOCH)
            .prop_map(datetime_from_epoch_secs)
            .boxed()
    }
}

#[cfg(feature = "proptest")]
impl<const MIN_SECS_SINCE_EPOCH: i64, const MAX_SECS_SINCE_EPOCH: i64> ArbitraryDateTime
    for DateTimeInRange<MIN_SECS_SINCE_EPOCH, MAX_SECS_SINCE_EPOCH>
{
    type Strategy = proptest::strategy::BoxedStrategy<DateTime<Utc>>;

    #[inline]
    fn arbitrary_datetime() -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        let () = Self::VALID;
        (MIN_SECS_SINCE_EPOCH..=MAX_SECS_SINCE_EPOCH)
            .prop_map(datetime_from_epoch_secs)
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

    use chrono::{DateTime, TimeZone, Utc};

    use crate::Refined;

    use super::{DateTimeAtLeast, DateTimeAtMost, DateTimeError, DateTimeInRange};

    fn at(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
            .unwrap()
    }

    // ─── DateTimeAtLeast. ─────────────────────────────────────────

    #[test]
    fn datetime_at_least_admits_endpoint() {
        type Since2024 = DateTimeAtLeast<1_704_067_200>;
        let r: Refined<DateTime<Utc>, Since2024> =
            Refined::try_new(at(2024, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!(*r.as_inner(), at(2024, 1, 1, 0, 0, 0));
    }

    #[test]
    fn datetime_at_least_admits_above() {
        type Since2024 = DateTimeAtLeast<1_704_067_200>;
        let r: Refined<DateTime<Utc>, Since2024> =
            Refined::try_new(at(2024, 5, 28, 12, 0, 0)).unwrap();
        assert_eq!(r.as_inner().timestamp(), 1_716_897_600);
    }

    #[test]
    fn datetime_at_least_rejects_below() {
        type Since2024 = DateTimeAtLeast<1_704_067_200>;
        let bad = at(2023, 12, 31, 23, 59, 59);
        let res: Result<Refined<DateTime<Utc>, Since2024>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateTimeError::OutOfRange { value: bad });
    }

    // ─── DateTimeAtMost. ──────────────────────────────────────────

    #[test]
    fn datetime_at_most_admits_endpoint() {
        type Until2030 = DateTimeAtMost<1_893_456_000>;
        let r: Refined<DateTime<Utc>, Until2030> =
            Refined::try_new(at(2030, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!(*r.as_inner(), at(2030, 1, 1, 0, 0, 0));
    }

    #[test]
    fn datetime_at_most_admits_below() {
        type Until2030 = DateTimeAtMost<1_893_456_000>;
        let r: Refined<DateTime<Utc>, Until2030> =
            Refined::try_new(at(2024, 5, 28, 12, 0, 0)).unwrap();
        assert_eq!(r.as_inner().timestamp(), 1_716_897_600);
    }

    #[test]
    fn datetime_at_most_rejects_above() {
        type Until2030 = DateTimeAtMost<1_893_456_000>;
        let bad = at(2030, 1, 1, 0, 0, 1);
        let res: Result<Refined<DateTime<Utc>, Until2030>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateTimeError::OutOfRange { value: bad });
    }

    // ─── DateTimeInRange. ─────────────────────────────────────────

    #[test]
    fn datetime_in_range_admits_midpoint() {
        type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
        let r: Refined<DateTime<Utc>, ThisDecade> =
            Refined::try_new(at(2027, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!(*r.as_inner(), at(2027, 1, 1, 0, 0, 0));
    }

    #[test]
    fn datetime_in_range_admits_lower_endpoint() {
        type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
        let r: Refined<DateTime<Utc>, ThisDecade> =
            Refined::try_new(at(2024, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!(*r.as_inner(), at(2024, 1, 1, 0, 0, 0));
    }

    #[test]
    fn datetime_in_range_admits_upper_endpoint() {
        type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
        let r: Refined<DateTime<Utc>, ThisDecade> =
            Refined::try_new(at(2030, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!(*r.as_inner(), at(2030, 1, 1, 0, 0, 0));
    }

    #[test]
    fn datetime_in_range_rejects_below_lower() {
        type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
        let bad = at(2023, 12, 31, 23, 59, 59);
        let res: Result<Refined<DateTime<Utc>, ThisDecade>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateTimeError::OutOfRange { value: bad });
    }

    #[test]
    fn datetime_in_range_rejects_above_upper() {
        type ThisDecade = DateTimeInRange<1_704_067_200, 1_893_456_000>;
        let bad = at(2030, 1, 1, 0, 0, 1);
        let res: Result<Refined<DateTime<Utc>, ThisDecade>, _> = Refined::try_new(bad);
        assert_eq!(res.unwrap_err(), DateTimeError::OutOfRange { value: bad });
    }

    // ─── Display. ─────────────────────────────────────────────────

    #[test]
    fn display_out_of_range() {
        let err = DateTimeError::OutOfRange {
            value: at(2023, 12, 31, 23, 59, 59),
        };
        assert_eq!(
            err.to_string(),
            "datetime 2023-12-31 23:59:59 UTC out of admissible range",
        );
    }

    // ─── SchemaRule: the constructive descriptions. ────────────────

    use crate::schema::{Bound, Scalar, ScalarKind, Schema, SchemaRule};

    fn datetime_interval(lo: Bound, hi: Bound) -> Schema {
        Schema::interval(ScalarKind::DateTime, lo, hi)
    }

    #[test]
    fn schema_reads_the_same_bounds_refine_reads() {
        assert_eq!(
            <DateTimeAtLeast<1_704_067_200> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(
                Bound::Inclusive(Scalar::Int(1_704_067_200)),
                Bound::Unbounded,
            ),
        );
        assert_eq!(
            <DateTimeAtLeast<0> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(Bound::Inclusive(Scalar::Int(0)), Bound::Unbounded),
        );
        assert_eq!(
            <DateTimeAtMost<1_893_456_000> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(
                Bound::Unbounded,
                Bound::Inclusive(Scalar::Int(1_893_456_000)),
            ),
        );
        assert_eq!(
            <DateTimeAtMost<0> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(Bound::Unbounded, Bound::Inclusive(Scalar::Int(0))),
        );
        assert_eq!(
            <DateTimeInRange<1_704_067_200, 1_893_456_000> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(
                Bound::Inclusive(Scalar::Int(1_704_067_200)),
                Bound::Inclusive(Scalar::Int(1_893_456_000)),
            ),
        );
        assert_eq!(
            <DateTimeInRange<0, 60> as SchemaRule<DateTime<Utc>>>::schema(),
            datetime_interval(
                Bound::Inclusive(Scalar::Int(0)),
                Bound::Inclusive(Scalar::Int(60)),
            ),
        );
    }

    #[cfg(feature = "proptest")]
    mod schema_cross_checks {
        use super::super::{DateTimeAtLeast, DateTimeAtMost, DateTimeInRange};
        use crate::schema::{Scalar, ScalarKind};
        use crate::testing::prop_schema_cross_check;
        use chrono::{DateTime, Utc};

        fn embed_datetime(value: &DateTime<Utc>) -> (ScalarKind, Scalar) {
            (
                ScalarKind::DateTime,
                Scalar::Int(i128::from(value.timestamp())),
            )
        }

        #[expect(
            clippy::return_and_then,
            reason = "the branch-free and_then chain keeps this fn fully covered: a `?` \
                      would add a None arm no boundary candidate reaches"
        )]
        fn extract_datetime(_kind: ScalarKind, scalar: Scalar) -> Option<DateTime<Utc>> {
            scalar
                .as_int()
                .and_then(|widened| i64::try_from(widened).ok())
                .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0))
        }

        /// Schema endpoints pass refine and strategy samples are
        /// schema members, two instantiations per datetime rule.
        #[test]
        fn schema_cross_checks_datetime_rules() {
            prop_schema_cross_check::<DateTime<Utc>, DateTimeAtLeast<1_704_067_200>>(
                embed_datetime,
                extract_datetime,
            );
            prop_schema_cross_check::<DateTime<Utc>, DateTimeAtLeast<0>>(
                embed_datetime,
                extract_datetime,
            );
            prop_schema_cross_check::<DateTime<Utc>, DateTimeAtMost<1_893_456_000>>(
                embed_datetime,
                extract_datetime,
            );
            prop_schema_cross_check::<DateTime<Utc>, DateTimeAtMost<0>>(
                embed_datetime,
                extract_datetime,
            );
            prop_schema_cross_check::<DateTime<Utc>, DateTimeInRange<1_704_067_200, 1_893_456_000>>(
                embed_datetime,
                extract_datetime,
            );
            prop_schema_cross_check::<DateTime<Utc>, DateTimeInRange<0, 60>>(
                embed_datetime,
                extract_datetime,
            );
        }
    }

    // ─── Arbitrary strategy soundness. ────────────────────────────

    #[cfg(feature = "proptest")]
    proptest::proptest! {
        #[test]
        fn arbitrary_datetime_at_least_value_in_range(
            r in proptest::arbitrary::any::<
                Refined<DateTime<Utc>, DateTimeAtLeast<1_704_067_200>>,
            >()
        ) {
            proptest::prop_assert!(r.as_inner().timestamp() >= 1_704_067_200);
        }

        #[test]
        fn arbitrary_datetime_at_most_value_in_range(
            r in proptest::arbitrary::any::<
                Refined<DateTime<Utc>, DateTimeAtMost<1_893_456_000>>,
            >()
        ) {
            proptest::prop_assert!(r.as_inner().timestamp() <= 1_893_456_000);
        }

        #[test]
        fn arbitrary_datetime_in_range_value_in_range(
            r in proptest::arbitrary::any::<
                Refined<DateTime<Utc>, DateTimeInRange<1_704_067_200, 1_893_456_000>>,
            >()
        ) {
            let secs = r.as_inner().timestamp();
            proptest::prop_assert!((1_704_067_200..=1_893_456_000).contains(&secs));
        }
    }
}
