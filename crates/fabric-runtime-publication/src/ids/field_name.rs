//! Names a field within a collection.

use std::fmt;

/// The name of a field on a collection, as it appears in a discriminator
/// column, a catalogue's `key_field`, or a catalogue's `queryable_fields`.
///
/// The canonical type is `fabric_connector::FieldName`. See
/// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
/// rather than depending on the crate that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldName(String);

impl FieldName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "field name";

    /// Parses a field name.
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

impl fmt::Display for FieldName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for FieldName {
    type Error = fabric_core::IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<FieldName> for String {
    fn from(value: FieldName) -> Self {
        value.0
    }
}
