//! What a subject *is* to a resource.

use std::fmt;

use crate::ids::slug::parse_identifier;
use crate::IdentifierError;

/// A relation a subject can hold on a resource — `viewer`, `editor`, `owner`.
///
/// # Why this is shared between the planes
///
/// The control plane writes these into a client's desired state; the runtime
/// plane names one on every authorization check. Two definitions of `viewer`
/// would drift without ever failing a build, and the failure would surface as
/// somebody being refused access an operator had granted — the same argument
/// that put [`OperationKind`](crate::OperationKind) and
/// [`LogicalResourceName`](crate::LogicalResourceName) here.
///
/// # A noun, not a permission
///
/// A relation says how a subject is *related* to a resource. Which operations
/// that permits is declared separately (ADR 0013), so widening what an editor
/// may do never requires inventing a new word for an editor.
///
/// ```
/// # use fabric_core::RelationName;
/// assert!(RelationName::try_new("billing_admin").is_ok());
/// assert!(RelationName::try_new("has space").is_err());
/// # Ok::<(), fabric_core::IdentifierError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelationName(String);

impl RelationName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "relation name";

    /// Parses a relation name.
    ///
    /// Takes the permissive identifier rule rather than the DNS one:
    /// `billing_admin` is a reasonable relation and reads better than
    /// `billing-admin` to the people who write these documents.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 63
    /// bytes, does not begin with an ASCII letter, or contains anything
    /// outside ASCII letters, digits, hyphens, and underscores.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        parse_identifier(Self::KIND, value.as_ref()).map(Self)
    }

    /// Borrows the name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelationName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RelationName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RelationName> for String {
    fn from(value: RelationName) -> Self {
        value.0
    }
}
