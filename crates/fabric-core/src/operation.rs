//! What a caller is trying to do to a resource.
//!
//! # Why this is shared rather than the Data API's own
//!
//! It began in `fabric-data-api`, which is where it is enforced, and that was
//! right while enforcement was the only thing that named it. It is no longer:
//! a client's desired state now declares which relations permit which
//! operations, and that document is written by the control plane.
//!
//! The two planes must not depend on each other (ADR 0008) and must still mean
//! the same thing by "read". A vocabulary duplicated on both sides of that
//! boundary would drift silently — the control plane would write `modify`, the
//! runtime plane would look for `update`, and nothing would fail until a
//! caller was refused something an operator had granted. So it lives in the
//! crate the two are allowed to share, beside [`LogicalResourceName`], which
//! is in this crate for the same reason.
//!
//! [`LogicalResourceName`]: crate::LogicalResourceName

/// The kind of operation being attempted.
///
/// Five, and deliberately not more. These are the operations the Data API
/// exposes; a finer distinction belongs to the resource's own definition
/// rather than to a vocabulary two planes have to agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// Fetch one row by key.
    Read,
    /// Fetch many rows.
    List,
    /// Insert rows.
    Create,
    /// Modify rows.
    Update,
    /// Remove rows.
    Delete,
}

impl OperationKind {
    /// Every operation, in a stable order.
    ///
    /// Ordered so that a rendered document and a generated authorization model
    /// list them the same way every time; an unstable order would turn a
    /// no-op reconciliation into a diff.
    pub const ALL: [Self; 5] = [Self::Read, Self::List, Self::Create, Self::Update, Self::Delete];

    /// A stable name for telemetry (§29), a document, and a relation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::List => "list",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }

    /// Whether the operation modifies data.
    #[must_use]
    pub const fn is_write(self) -> bool {
        matches!(self, Self::Create | Self::Update | Self::Delete)
    }
}

impl std::fmt::Display for OperationKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
