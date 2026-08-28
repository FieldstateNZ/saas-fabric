//! The tenant identifier — the single value the whole runtime plane pivots on.

use std::fmt;

use crate::ids::slug::parse_dns_label;
use crate::IdentifierError;

/// A validated tenant identifier, as carried by the canonical `tenant_id`
/// claim.
///
/// Holding one of these is a proof, not a hint. The only way to build a
/// `TenantId` is [`TenantId::try_new`], which enforces the DNS-label character
/// set, so code downstream of tenant resolution may use the value in a schema
/// name, a connection pool key, or a metrics label without re-checking it.
///
/// That guarantee is why the type exists at all. The tenant id originates in a
/// bearer token — that is, from outside the trust boundary — and it ends up
/// being interpolated into SQL identifiers when a tenant is placed on a shared
/// database with per-tenant schemas. A raw `String` would put a
/// caller-influenced value one careless format string away from a SQL
/// identifier; this newtype closes that path at the point of parsing.
///
/// # Examples
///
/// ```
/// use fabric_core::TenantId;
///
/// let tenant = TenantId::try_new("acme")?;
/// assert_eq!(tenant.as_str(), "acme");
///
/// // Anything that could escape a SQL identifier is rejected up front.
/// assert!(TenantId::try_new("acme\"; drop schema public --").is_err());
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TenantId(String);

impl TenantId {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "tenant id";

    /// Parses a tenant identifier, enforcing the DNS-label character set.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 63 bytes,
    /// contains anything outside lowercase ASCII letters, digits, and hyphens,
    /// or begins or ends with a hyphen.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        parse_dns_label(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for TenantId {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<TenantId> for String {
    fn from(value: TenantId) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialising_runs_the_same_validation_as_the_constructor() {
        // The registry loads bindings from JSON, so a bad tenant id must be
        // rejected at the deserialisation boundary too — not just when a
        // constructor happens to be called.
        let error = serde_json::from_str::<TenantId>("\"Acme\"").unwrap_err();
        assert!(error.to_string().contains("disallowed character"));
    }

    #[test]
    fn round_trips_through_json() {
        let tenant = TenantId::try_new("acme").unwrap();
        let encoded = serde_json::to_string(&tenant).unwrap();
        assert_eq!(encoded, "\"acme\"");
        assert_eq!(serde_json::from_str::<TenantId>(&encoded).unwrap(), tenant);
    }
}
