//! The decoded claim set, before it is interpreted into a tenant identity.

mod numeric_date;

use serde_json::{Map, Value};

use crate::claims::numeric_date::to_numeric_date;

/// The claims carried by a bearer token, as a raw JSON object.
///
/// Deliberately untyped. Claim names are configurable (§10) and deployments add
/// their own claims, so a fixed struct would either reject valid tokens or need
/// a `HashMap` escape hatch anyway. [`TenantIdentity`](crate::TenantIdentity)
/// is the typed view that comes *out* of this.
#[derive(Debug, Clone, Default)]
pub struct TokenClaims(Map<String, Value>);

impl TokenClaims {
    /// Wraps a decoded claim object.
    #[must_use]
    pub const fn new(claims: Map<String, Value>) -> Self {
        Self(claims)
    }

    /// Reads a claim as a string, returning `None` when it is absent or is not
    /// a JSON string.
    ///
    /// A non-string claim is treated as absent rather than coerced. A
    /// `tenant_id` of `42` is a misconfigured identity provider, and quietly
    /// stringifying it would hide that.
    #[must_use]
    pub fn string(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(Value::as_str)
    }

    /// Reads a claim as a list of strings.
    ///
    /// Accepts both shapes that appear in the wild: a JSON array of strings
    /// (`"roles": ["user", "admin"]`) and a single space-delimited string
    /// (`"scope": "read write"`). Non-string array entries are skipped.
    #[must_use]
    pub fn string_list(&self, name: &str) -> Vec<String> {
        match self.0.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
            Some(Value::String(joined)) => joined.split_whitespace().map(ToOwned::to_owned).collect(),
            _ => Vec::new(),
        }
    }

    /// Reads a numeric claim, such as `exp` or `nbf`, as seconds since the Unix
    /// epoch.
    ///
    /// # Why this is not `Value::as_i64`
    ///
    /// RFC 7519 §2 defines a `NumericDate` as a JSON *number* and explicitly
    /// permits a non-integer value, so `{"exp":1000.5}` is a perfectly legal
    /// token. `serde_json`'s integer accessors return `None` for every float,
    /// and `None` here means "no such claim" — which the validity-window checks
    /// accept, because a token need not carry one. A fractional `exp` therefore
    /// used to switch off the expiry check it was supposed to impose, and a
    /// fractional `nbf` did the same at the other end of the window.
    ///
    /// # What `None` means
    ///
    /// Only that the claim is absent, or is not a JSON number at all. A
    /// `{"exp":"soon"}` is a misconfigured identity provider, and is treated as
    /// absent rather than coerced, for the same reason [`Self::string`] refuses
    /// to stringify a numeric `tenant_id`. Every value that *is* a number
    /// yields a second, so no number a token carries can quietly stop
    /// constraining anything.
    #[must_use]
    pub fn unix_seconds(&self, name: &str) -> Option<u64> {
        let value = self.0.get(name)?;

        // Whole values within `u64` pass through exactly. `as_f64` then covers
        // both remaining cases in one step: a fractional or out-of-range number
        // becomes a float, and a non-number becomes the `None` above.
        if let Some(seconds) = value.as_u64() {
            return Some(seconds);
        }

        value.as_f64().map(to_numeric_date)
    }

    /// Borrows the underlying claim object.
    ///
    /// Provided for diagnostics. Do not use it to reach a tenant id — that must
    /// go through the configured claim name so there stays exactly one way to
    /// determine the tenant.
    #[must_use]
    pub const fn raw(&self) -> &Map<String, Value> {
        &self.0
    }
}

impl From<Map<String, Value>> for TokenClaims {
    fn from(claims: Map<String, Value>) -> Self {
        Self(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(json: &str) -> TokenClaims {
        TokenClaims::new(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn reads_a_string_claim() {
        assert_eq!(
            claims(r#"{"tenant_id":"acme"}"#).string("tenant_id"),
            Some("acme")
        );
    }

    #[test]
    fn a_non_string_tenant_claim_reads_as_absent_rather_than_being_coerced() {
        assert_eq!(claims(r#"{"tenant_id":42}"#).string("tenant_id"), None);
    }

    #[test]
    fn reads_roles_from_a_json_array() {
        assert_eq!(
            claims(r#"{"roles":["user","admin"]}"#).string_list("roles"),
            ["user", "admin"]
        );
    }

    #[test]
    fn reads_scopes_from_a_space_delimited_string() {
        assert_eq!(
            claims(r#"{"scope":"read write"}"#).string_list("scope"),
            ["read", "write"]
        );
    }

    #[test]
    fn a_missing_list_claim_is_empty_rather_than_an_error() {
        assert!(claims("{}").string_list("roles").is_empty());
    }

    #[test]
    fn reads_a_whole_numeric_date() {
        assert_eq!(claims(r#"{"exp":1000}"#).unix_seconds("exp"), Some(1_000));
    }

    #[test]
    fn reads_a_numeric_date_written_as_a_whole_float() {
        // The shape the adversarial review used. Spec-legal, and previously
        // read as absent, which switched the check off entirely.
        assert_eq!(claims(r#"{"exp":1000.0}"#).unix_seconds("exp"), Some(1_000));
    }

    #[test]
    fn rounds_a_fractional_numeric_date_half_away_from_zero() {
        // Matching `jsonwebtoken`'s `value.round() as u64` exactly is what
        // keeps the two postures agreeing on the second a token expires.
        assert_eq!(claims(r#"{"exp":1000.4}"#).unix_seconds("exp"), Some(1_000));
        assert_eq!(claims(r#"{"exp":1000.5}"#).unix_seconds("exp"), Some(1_001));
        assert_eq!(claims(r#"{"exp":1000.6}"#).unix_seconds("exp"), Some(1_001));
    }

    #[test]
    fn a_date_before_the_epoch_clamps_to_zero_rather_than_reading_as_absent() {
        // Zero is honest at both ends of the window: an instant in 1969 has
        // passed, so such an `exp` is expired and such an `nbf` has arrived.
        assert_eq!(claims(r#"{"exp":-1}"#).unix_seconds("exp"), Some(0));
        assert_eq!(claims(r#"{"nbf":-1.5}"#).unix_seconds("nbf"), Some(0));
        assert_eq!(claims(r#"{"exp":-1e30}"#).unix_seconds("exp"), Some(0));
    }

    #[test]
    fn a_date_beyond_the_representable_range_clamps_to_the_maximum() {
        // The saturating cast doing the work that a wrap or a panic would not.
        assert_eq!(claims(r#"{"nbf":1e30}"#).unix_seconds("nbf"), Some(u64::MAX));
    }

    #[test]
    fn the_largest_whole_second_survives_without_passing_through_a_float() {
        // `u64::MAX` has no exact `f64`, so routing it through one would round
        // *up* and saturate. The integer path is what keeps it exact.
        assert_eq!(
            claims(r#"{"exp":18446744073709551615}"#).unix_seconds("exp"),
            Some(u64::MAX)
        );
    }

    #[test]
    fn a_non_numeric_date_reads_as_absent_rather_than_being_coerced() {
        assert_eq!(claims(r#"{"exp":"soon"}"#).unix_seconds("exp"), None);
        assert_eq!(claims(r#"{"exp":null}"#).unix_seconds("exp"), None);
        assert_eq!(claims(r#"{"exp":true}"#).unix_seconds("exp"), None);
        assert_eq!(claims("{}").unix_seconds("exp"), None);
    }

    #[test]
    fn a_non_finite_numeric_date_cannot_reach_the_reader_at_all() {
        // Why `to_numeric_date` has no NaN or infinity branch: JSON has no
        // literal for either, an overflowing exponent is a parse error rather
        // than an infinity, and `Number` refuses to hold one.
        assert!(serde_json::from_str::<Value>("1e400").is_err());
        assert!(serde_json::from_str::<Value>("-1e400").is_err());
        assert!(serde_json::Number::from_f64(f64::NAN).is_none());
        assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
    }
}
