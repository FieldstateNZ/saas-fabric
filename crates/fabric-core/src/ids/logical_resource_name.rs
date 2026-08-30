//! The name the platform gives a resource, such as `customers`.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// The name of a logical resource — `customers`, `orders`, `auditEvents`.
///
/// # The platform's resource vocabulary, not one subsystem's
///
/// This is what a resource is *called* everywhere in SaaS Fabric, and it has
/// three consumers rather than an owner and some borrowers:
///
/// - a client's **desired state** declares which relations exist on it
///   (ADR 0013)
/// - the **Data API** resolves it through its catalogue into a logical data
///   source and then a physical table
/// - **authorization** names it as the type half of an object a decision is
///   about (ADR 0016)
///
/// It lived here from the start because the Data API needed it first, and its
/// documentation used to describe only that use — which would have made
/// authorization read as subordinate to the Data API for no better reason than
/// the order the two were built in. Nothing about the name is Data-API
/// specific: a resource is a platform concept, and the catalogue is one thing
/// that resolves it.
///
/// # Why it is validated
///
/// The name reaches a SQL identifier position in one consumer and an
/// authorization object in another, and it arrives from a request path
/// (`POST /data/customers`) in the first. It is checked once, on the way in,
/// so that no consumer has to check it again.
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
