//! Serde round-trip for `closed_set!`-generated enums.
//!
//! The macro emits `Serialize`/`Deserialize` impls behind whittle's
//! `serde` feature: the wire shape is the **plain wire string** —
//! the same shape the provider sent, no enum-variant wrapping — and
//! deserialization routes through `closed_set::parse`, so untrusted
//! ingress is gated by the same boundary morphism as every other
//! construction path and rejections carry the domain diagnostics.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "integration test: unwrap keeps the focus on the API"
)]

use serde::{Deserialize, Serialize};
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

/// A provider payload embedding the closed-set enum as a field.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Account {
    /// Current activity status.
    status: ActivityStatus,
}

#[test]
fn round_trips_the_plain_wire_string() {
    let account = Account {
        status: ActivityStatus::Active,
    };
    let json = serde_json::to_string(&account).unwrap();
    assert_eq!(json, r#"{"status":"active"}"#);

    let back: Account = serde_json::from_str(&json).unwrap();
    assert_eq!(back, account);
}

#[test]
fn rejects_non_members_at_deserialize_time_with_domain_diagnostics() {
    let err = serde_json::from_str::<Account>(r#"{"status":"actve"}"#).unwrap_err();
    assert!(
        err.to_string()
            .contains(r#"invalid value "actve": expected one of "active", "inactive""#),
    );
}

#[test]
fn rejects_non_string_wire_values_naming_the_expected_set() {
    let err = serde_json::from_str::<Account>(r#"{"status":7}"#).unwrap_err();
    assert!(err.to_string().contains(r#"one of "active", "inactive""#));
}
