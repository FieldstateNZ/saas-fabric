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

    /// Whether the operation is asked about a resource's collection rather
    /// than one of its records (ADR 0027 §2a).
    ///
    /// `list` and `create` name no record. The authorization model defines
    /// them only on the collection object, and `read`, `update` and `delete`
    /// only on a record, so the two partitions are one fact both planes read
    /// here rather than two copies that could disagree.
    #[must_use]
    pub const fn is_collection_operation(self) -> bool {
        matches!(self, Self::List | Self::Create)
    }

    /// The computed relation that answers whether the operation is permitted:
    /// `can_<operation>`.
    ///
    /// ADR 0013 maps a client's declared relations onto direct relations plus
    /// one computed relation per operation. The control plane writes these
    /// names into the model and the runtime plane asks them, so they are
    /// spelled once, here.
    #[must_use]
    pub const fn computed_relation(self) -> &'static str {
        match self {
            Self::Read => "can_read",
            Self::List => "can_list",
            Self::Create => "can_create",
            Self::Update => "can_update",
            Self::Delete => "can_delete",
        }
    }
}

impl std::fmt::Display for OperationKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationName;

    #[test]
    fn list_and_create_are_collection_operations_and_nothing_else_is() {
        let collection: Vec<OperationKind> = OperationKind::ALL
            .into_iter()
            .filter(|operation| operation.is_collection_operation())
            .collect();

        assert_eq!(collection, [OperationKind::List, OperationKind::Create]);
    }

    #[test]
    fn each_operation_asks_its_own_computed_relation() {
        let relations: Vec<&str> = OperationKind::ALL
            .into_iter()
            .map(OperationKind::computed_relation)
            .collect();

        assert_eq!(
            relations,
            ["can_read", "can_list", "can_create", "can_update", "can_delete"]
        );
    }

    #[test]
    fn every_computed_relation_is_can_and_the_operation_and_a_valid_relation_name() {
        for operation in OperationKind::ALL {
            let relation = operation.computed_relation();

            assert_eq!(relation, format!("can_{operation}"));
            assert!(RelationName::try_new(relation).is_ok(), "{relation}");
        }
    }
}
