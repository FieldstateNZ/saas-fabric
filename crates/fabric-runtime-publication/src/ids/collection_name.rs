//! Names the physical collection a published catalogue entry points at.

use std::fmt;

/// The name of a physical collection — a table, a view, a document
/// collection — as it appears in a published
/// [`ResourceDefinitionDocument`](crate::ResourceDefinitionDocument).
///
/// The canonical type is `fabric_connector::CollectionName`. See
/// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
/// rather than depending on the crate that owns it. ADR 0018, Decision part 1
/// names this type explicitly alongside `ConnectorId`, `ConnectionName`, and
/// `FieldName` as one the producer must validate itself: a `collection` value
/// the producer accepted unchecked could fail the consumer's own parse at
/// startup, taking the whole file — and, for the catalogue, the process —
/// down with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CollectionName(String);

impl CollectionName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "collection name";

    /// Parses a collection name.
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

impl fmt::Display for CollectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for CollectionName {
    type Error = fabric_core::IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<CollectionName> for String {
    fn from(value: CollectionName) -> Self {
        value.0
    }
}
