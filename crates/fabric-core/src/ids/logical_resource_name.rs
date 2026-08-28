//! The logical resource name an application addresses, such as `customers`.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// The name of a logical data resource — `customers`, `orders`, `auditEvents`.
///
/// Applications address these names and nothing else. A logical resource is
/// resolved by the Data API's catalogue into a logical data source, and only
/// then into a physical table on a physical server. Because the name arrives in
/// the request path (`POST /data/customers`), it is validated on the way in so
/// that no caller-supplied string ever reaches a SQL identifier position.
///
/// # Examples
///
/// ```
/// use fabric_core::LogicalResourceName;
///
/// let customers = LogicalResourceName::try_new("customers")?;
/// assert_eq!(customers.as_str(), "customers");
///
/// // The specification's own catalogue uses camelCase, so it is permitted.
/// assert!(LogicalResourceName::try_new("auditEvents").is_ok());
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LogicalResourceName(String);

impl LogicalResourceName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "logical resource name";

    /// Parses a logical resource name.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 63 bytes,
    /// does not begin with an ASCII letter, or contains anything outside ASCII
    /// letters, digits, hyphens, and underscores.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        parse_identifier(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalResourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for LogicalResourceName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<LogicalResourceName> for String {
    fn from(value: LogicalResourceName) -> Self {
        value.0
    }
}
