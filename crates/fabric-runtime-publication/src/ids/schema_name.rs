//! Names the schema a published tenant binding isolates into.

use std::fmt;

/// A schema qualifier, as it appears in a published
/// [`IsolationModelDocument::Schema`](crate::IsolationModelDocument::Schema).
///
/// The canonical type is `fabric_connector::SchemaName`. See
/// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
/// rather than depending on the crate that owns it. ADR 0018, Decision part 1
/// names this type explicitly alongside `ConnectorId`, `ConnectionName`, and
/// `FieldName` as one the producer must validate itself, so that a value
/// either side accepts is a value the other accepts too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SchemaName(String);

impl SchemaName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "schema name";

    /// Parses a schema name.
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

impl fmt::Display for SchemaName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for SchemaName {
    type Error = fabric_core::IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<SchemaName> for String {
    fn from(value: SchemaName) -> Self {
        value.0
    }
}
