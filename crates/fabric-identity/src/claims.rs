//! The decoded claim set, before it is interpreted into a tenant identity.

use serde_json::{Map, Value};

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
    #[must_use]
    pub fn unix_seconds(&self, name: &str) -> Option<i64> {
        self.0.get(name).and_then(Value::as_i64)
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
}
