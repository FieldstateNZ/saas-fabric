//! The identity of a configured, physical DataSource resource.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// Identifies a **DataSource** — a configured, reusable physical data
/// destination.
///
/// # Where this sits
///
/// ```text
/// LogicalResourceName      customers, orders, auditEvents
///         ↓ catalogue
/// LogicalDataSourceName    primary, audit, analytics
///         ↓ tenant binding
/// DataSourceId             sql-au-east-03, shared-postgres-02   ← this type
///         ↓ registry
/// DataSource               the configured physical resource
///         ↓
/// Connector
/// ```
///
/// This type is **a physical resource**, not intent. It names something that
/// exists whether or not any tenant uses it: a database server, a cluster, a
/// schema-hosting instance. It is provisioned, monitored, patched and retired.
///
/// A DataSource is **shared** — many tenants point at `shared-postgres-02`,
/// which is what keeps connection counts bounded (§22). Everything physical
/// (connector, connection, pool, region, placement) belongs to the DataSource,
/// never to the tenants bound to it.
///
/// # This is the only identifier a DataSource has
///
/// There is deliberately no second "DataSource name" concept. One configured
/// resource, one canonical id.
///
/// # Applications never see this
///
/// Not the type, not the value. A `DataSourceId` reaching an application would
/// leak exactly the placement detail §2 and §26 keep behind the Data API, so it
/// appears in internal telemetry only.
///
/// # Examples
///
/// ```
/// use fabric_core::DataSourceId;
///
/// let source = DataSourceId::try_new("sql-au-east-03")?;
/// assert_eq!(source.as_str(), "sql-au-east-03");
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DataSourceId(String);

impl DataSourceId {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "data source id";

    /// Parses a DataSource identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 63 bytes,
    /// does not begin with an ASCII letter, or contains anything outside ASCII
    /// letters, digits, hyphens, and underscores.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        parse_identifier(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DataSourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for DataSourceId {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<DataSourceId> for String {
    fn from(value: DataSourceId) -> Self {
        value.0
    }
}
