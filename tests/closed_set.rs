//! Closed-set enums via the `closed_set!` macro.
//!
//! One declaration — variants paired with their wire strings — and
//! everything else is derived from the `MEMBERS` table it generates:
//! `FromStr`/`TryFrom<&str>` parse, `Display` as the wire form, the
//! standard derive set, and a typed error carrying the offending
//! value plus the expected set.
//!
//! Use this pattern when a provider field is a small fixed set of
//! nominal tokens (`"active"`/`"inactive"`, branch codes, ISO
//! currency codes) currently hand-rolled as a `match` in `try_new`.
//! Unlike `Refined<String, Rule>` the target is the enum itself —
//! the closed set IS the type, so consumers match exhaustively with
//! no `_ => unreachable!()` arms.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use whittle::closed_set;

closed_set! {
    /// Account activity status as reported by the provider.
    pub enum ActivityStatus {
        /// The account is in active use.
        Active = "active",
        /// The account is dormant.
        Inactive = "inactive",
    }
}

#[test]
fn parses_members_and_displays_the_wire_form() {
    // Admit: `FromStr` is the boundary morphism.
    let status: ActivityStatus = "active".parse().unwrap();
    assert_eq!(status, ActivityStatus::Active);

    // `TryFrom<&str>` is the same morphism.
    let inactive = ActivityStatus::try_from("inactive").unwrap();
    assert_eq!(inactive, ActivityStatus::Inactive);

    // `Display` reconstructs the wire form losslessly.
    assert_eq!(status.to_string(), "active");
    assert_eq!(inactive.to_string(), "inactive");
}

#[test]
fn rejects_non_members_with_bounded_value_and_expected_set() {
    let err = "actve".parse::<ActivityStatus>().unwrap_err();
    assert_eq!(err.value(), "actve");
    assert_eq!(
        err.expected(),
        <ActivityStatus as whittle::ClosedSet>::MEMBERS
    );
    assert_eq!(
        err.to_string(),
        r#"invalid value "actve": expected one of "active", "inactive""#,
    );
}

#[test]
fn round_trips_through_the_module_fns() {
    // The macro's impls forward to the generic module fns, which
    // remain directly usable.
    let wire = whittle::closed_set::as_str(ActivityStatus::Inactive);
    assert_eq!(wire, "inactive");
    let back: ActivityStatus = whittle::closed_set::parse(wire).unwrap();
    assert_eq!(back, ActivityStatus::Inactive);
}
