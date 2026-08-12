//! The logical data source an application's resource is bound to.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// The name of a **logical** data source — `primary`, `audit`, `analytics`.
///
/// # Where this sits
///
/// Three names in the platform look similar and mean entirely different
/// things. The chain runs:
///
/// ```text
/// LogicalResourceName      customers, orders, auditEvents
///         ↓ catalogue
/// LogicalDataSourceName    primary, audit, analytics      ← this type
///         ↓ tenant binding
/// DataSourceId             sql-au-east-03, shared-postgres-02
///         ↓ registry
/// DataSource               the configured physical resource
///         ↓
/// Connector
/// ```
///
/// This type is **intent**. It says *which pool of data* a resource belongs to,
/// not where that data lives. A catalogue entry declares that `customers` lives
/// in `primary`; each tenant's binding then says which
/// [`DataSourceId`](crate::DataSourceId) their `primary` currently resolves to.
///
/// That indirection is the whole point of §16: the same `primary` is a
/// dedicated Azure SQL database for one tenant and a schema on a shared
/// PostgreSQL cluster for another, and the application contract does not change.
///
/// # Examples
///
/// ```
/// use fabric_core::LogicalDataSourceName;
///
/// let primary = LogicalDataSourceName::try_new("primary")?;
/// assert_eq!(primary.as_str(), "primary");
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LogicalDataSourceName(String);

impl LogicalDataSourceName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "logical data source name";

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

impl fmt::Display for LogicalDataSourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for LogicalDataSourceName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<LogicalDataSourceName> for String {
    fn from(value: LogicalDataSourceName) -> Self {
        value.0
    }
}
