//! Identifies the connector a published DataSource executes against.

use std::fmt;

/// A connector identifier, as it appears in a published
/// [`DataSourceDocument`](crate::DataSourceDocument).
///
/// The canonical type is `fabric_connector::ConnectorId`, but that crate is
/// runtime plane and this crate may depend on nothing but `fabric-core` (see
/// `docs/architecture/crate-dependencies.md`). Re-declaring the newtype over
/// the same [`fabric_core::naming::parse_identifier`] rule means the two
/// copies cannot quietly drift apart: a value this crate accepts is a value
/// the runtime accepts, because both ask the identical question.
///
/// # Examples
///
/// ```
/// use fabric_runtime_publication::ConnectorId;
///
/// let connector = ConnectorId::try_new("postgres-au-east")?;
/// assert_eq!(connector.as_str(), "postgres-au-east");
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConnectorId(String);

impl ConnectorId {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "connector id";

    /// Parses a connector identifier.
    ///
    /// # Errors
    ///
    /// Returns [`fabric_core::IdentifierError`] if the value is empty, longer
    /// than 63 bytes, does not begin with an ASCII letter, or contains
    /// anything outside ASCII letters, digits, hyphens, and underscores.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, fabric_core::IdentifierError> {
        fabric_core::naming::parse_identifier(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for ConnectorId {
    type Error = fabric_core::IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ConnectorId> for String {
    fn from(value: ConnectorId) -> Self {
        value.0
    }
}
