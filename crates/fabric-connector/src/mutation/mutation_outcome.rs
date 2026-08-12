//! What comes back from a write.

use crate::Row;

/// The result of a [`MutationSpec`](crate::MutationSpec).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MutationOutcome {
    /// How many rows the operation affected.
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
