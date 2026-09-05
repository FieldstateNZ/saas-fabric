//! Names one of a connector's pre-configured connections.

use std::fmt;

/// The name of a connection a connector already holds configuration for, as
/// it appears in a published
/// [`ConnectionSelectorDocument`](crate::ConnectionSelectorDocument).
///
/// The canonical type is `fabric_connector::ConnectionName`. See
/// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
/// rather than depending on the crate that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConnectionName(String);

impl ConnectionName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "connection name";

    /// Parses a connection name.
    ///
    /// # Errors
    ///
    /// Returns [`fabric_core::IdentifierError`] if the value is empty, longer
    /// than 63 bytes, does not begin with an ASCII letter, or contains
    /// anything outside ASCII letters, digits, hyphens, and underscores.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, fabric_core::IdentifierError> {
        fabric_core::naming::parse_identifier(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for ConnectionName {
    type Error = fabric_core::IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ConnectionName> for String {
    fn from(value: ConnectionName) -> Self {
        value.0
    }
}
