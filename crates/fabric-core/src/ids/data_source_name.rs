//! The logical data-source name an application asks for, such as `primary`.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// The name of a *logical* data source — `primary`, `audit`, `analytics`.
///
/// This is intent, not infrastructure. The same `primary` resolves to a
/// dedicated Azure SQL database for one tenant and a schema on a shared
/// PostgreSQL cluster for another; the application contract does not change.
/// The physical side of that mapping lives in the tenant's runtime binding and
/// is never exposed through this type.
///
/// # Examples
///
/// ```
/// use fabric_core::DataSourceName;
///
/// let primary = DataSourceName::try_new("primary")?;
/// assert_eq!(primary.as_str(), "primary");
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DataSourceName(String);

impl DataSourceName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "data source name";

    /// Parses a logical data-source name.
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

impl fmt::Display for DataSourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for DataSourceName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<DataSourceName> for String {
    fn from(value: DataSourceName) -> Self {
        value.0
    }
}
