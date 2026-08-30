//! The name of a client's secret boundary.

use std::fmt;

use fabric_core::naming::parse_dns_label;
use fabric_core::IdentifierError;

/// A client's secret boundary — `acme`, `contoso`.
///
/// # Why the DNS rule
///
/// It becomes a path segment in a request to the secret store and a component
/// of that store's own naming. The narrow rule is what lets an adapter
/// interpolate it without escaping, and it is the same rule a realm name takes
/// — the two are usually the same string, and having them validated
/// differently would be a trap.
///
/// # It cannot be built from a request
///
/// There is no path by which a caller's body becomes one of these: the only
/// serde route is a client's desired state, which an operator writes through
/// the control plane and Git records. That is deliberate — see this module's
/// parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SecretNamespace(String);

impl SecretNamespace {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "secret namespace";

    /// Parses a namespace.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 63
    /// bytes, or contains anything outside lowercase ASCII letters, digits and
    /// interior hyphens.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        parse_dns_label(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the namespace as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for SecretNamespace {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<SecretNamespace> for String {
    fn from(value: SecretNamespace) -> Self {
        value.0
    }
}
