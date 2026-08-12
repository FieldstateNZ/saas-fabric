//! What comes back from a read.

use crate::Row;

/// The result of a [`QuerySpec`](crate::QuerySpec).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryOutcome {
    /// The rows returned, in the order the backend produced them.
    pub rows: Vec<Row>,

    /// The total number of matching rows ignoring paging, when the backend
    /// supplied it.
    ///
    /// `None` means "not counted", which is different from `Some(0)`. Counting
    /// can be expensive, so a connector is entitled to decline — callers must
    /// treat the absence as unknown rather than as zero.
    pub total_count: Option<u64>,
}

impl QueryOutcome {
    /// An outcome carrying rows and no count.
    #[must_use]
    pub const fn from_rows(rows: Vec<Row>) -> Self {
        Self {
            rows,
            total_count: None,
        }
    }

    /// How many rows were returned in this page.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether this page is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
