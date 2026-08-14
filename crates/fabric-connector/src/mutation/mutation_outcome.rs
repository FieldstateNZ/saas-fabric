//! What comes back from a write.

use crate::Row;

/// The result of a [`MutationSpec`](crate::MutationSpec).
///
/// # This is a report, not a guarantee
///
/// Both fields are whatever the backend said. A connector translating NDC
/// recovers them from a procedure's return value, whose shape NDC does not
/// define — so `affected_rows` is an *interpretation* of connector-specific
/// JSON, not a protocol-level fact.
///
/// Consumers must therefore reconcile it against the operation they sent rather
/// than relaying it. `fabric-data-api`'s `execution::write_integrity` is where
/// that happens for the Data API, and its rustdoc records what such a check can
/// and cannot establish.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MutationOutcome {
    /// How many rows the backend reported affecting.
    ///
    /// Not validated here: this type is the neutral shape of what a connector
    /// said, and a count larger than the request is a meaningful thing for a
    /// caller to be able to detect. Suppressing it at this layer would hide the
    /// evidence rather than the fault.
    pub affected_rows: u64,

    /// Rows the backend returned, for connectors and operations that support
    /// it — typically the inserted rows with server-generated keys filled in.
    pub returned_rows: Vec<Row>,
}

impl MutationOutcome {
    /// An outcome reporting only an affected-row count.
    #[must_use]
    pub const fn affected(affected_rows: u64) -> Self {
        Self {
            affected_rows,
            returned_rows: Vec::new(),
        }
    }

    /// An outcome carrying the rows the backend returned.
    #[must_use]
    pub fn with_rows(mut self, rows: Vec<Row>) -> Self {
        self.returned_rows = rows;
        self
    }
}
