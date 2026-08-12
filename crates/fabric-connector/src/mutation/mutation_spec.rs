//! A write operation, in neutral terms.

use crate::{CollectionName, ExecutionTarget, Filter, Row};

/// A write against one collection.
///
/// Like [`QuerySpec`](crate::QuerySpec), a mutation must be passed through
/// [`MutationSpec::for_target`] before execution. The stakes are higher here:
/// an unscoped read leaks another tenant's data, but an unscoped update or
/// delete *destroys* it.
#[derive(Debug, Clone, PartialEq)]
pub enum MutationSpec {
    /// Insert rows.
    Insert {
        /// The collection to insert into.
        collection: CollectionName,
        /// The rows to insert.
        rows: Vec<Row>,
    },

    /// Update rows matching a predicate.
    Update {
        /// The collection to update.
        collection: CollectionName,
        /// Which rows to update.
        filter: Option<Filter>,
        /// The fields to set.
        changes: Row,
    },

    /// Delete rows matching a predicate.
    Delete {
        /// The collection to delete from.
        collection: CollectionName,
        /// Which rows to delete.
        filter: Option<Filter>,
    },
}

impl MutationSpec {
    /// The collection this mutation touches.
    #[must_use]
    pub const fn collection(&self) -> &CollectionName {
        match self {
            Self::Insert { collection, .. }
            | Self::Update { collection, .. }
            | Self::Delete { collection, .. } => collection,
        }
    }

    /// A stable operation name for telemetry (§29).
    #[must_use]
    pub const fn operation_name(&self) -> &'static str {
        match self {
            Self::Insert { .. } => "insert",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
        }
    }

    /// Returns the mutation as it must actually be executed for a tenant.
    ///
    /// Under discriminator isolation this does two distinct things:
    ///
    /// - **Inserts** get the discriminator column *stamped onto every row*, so
    ///   a caller cannot create a row belonging to another tenant — whatever
    ///   the caller supplied for that column is overwritten, not merged.
    /// - **Updates and deletes** get the discriminator conjoined to their
    ///   predicate, so they can only ever reach this tenant's rows. A delete
    ///   with no predicate then deletes this tenant's rows, not the table.
    ///
    /// For dedicated-database and schema placements the mutation is returned
    /// unchanged, because the connection already cannot reach other tenants.
    #[must_use]
    pub fn for_target(&self, target: &ExecutionTarget) -> Self {
        let Some(tenant_predicate) = target.isolation().tenant_predicate() else {
            return self.clone();
        };

        match self {
            Self::Insert { collection, rows } => Self::Insert {
                collection: collection.clone(),
                rows: rows.iter().map(|row| stamp(row, target)).collect(),
            },
            Self::Update {
                collection,
                filter,
                changes,
            } => Self::Update {
                collection: collection.clone(),
                filter: Some(conjoin(filter.clone(), tenant_predicate)),
                // The discriminator is stamped here too: an update must not be
                // able to move a row out of this tenant.
                changes: stamp(changes, target),
            },
            Self::Delete { collection, filter } => Self::Delete {
                collection: collection.clone(),
                filter: Some(conjoin(filter.clone(), tenant_predicate)),
            },
        }
    }
}

/// Forces the discriminator column on a row to this tenant's value.
///
/// Overwrites rather than fills a gap. A caller that supplied its own value for
/// the discriminator column is either confused or hostile, and honouring it
/// would let one tenant write into another's data.
fn stamp(row: &Row, target: &ExecutionTarget) -> Row {
    match target.isolation() {
        crate::IsolationModel::Discriminator { column, value } => row
            .clone()
            .with(column.clone(), serde_json::Value::String(value.clone())),
        crate::IsolationModel::Database | crate::IsolationModel::Schema { .. } => row.clone(),
    }
}

/// Combines an optional caller predicate with the mandatory tenant predicate.
fn conjoin(caller: Option<Filter>, tenant: Filter) -> Filter {
    match caller {
        Some(filter) => filter.and(tenant),
        None => tenant,
    }
}
