//! One resource's relations, and what each of them permits.

use fabric_core::{LogicalResourceName, OperationKind};

use crate::RelationName;

/// The relations that can be held on one resource.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceAuthorization {
    /// The resource these relations are about.
    ///
    /// The same name the Data API's catalogue uses. Stated rather than
    /// inferred: a resource that is only ever implied cannot be read out of
    /// the document, and reconciliation has to name it.
    pub resource: LogicalResourceName,

    /// The relations somebody can hold on it.
    pub relations: Vec<Relation>,
}

/// One relation, and the operations holding it permits.
///
/// # Why these are two fields and not one
///
/// The tempting shape is a map from operation to the relations that may
/// perform it. It reads well and it is the wrong way round: an operator thinks
/// "what is an editor allowed to do", changes that answer, and expects to edit
/// one place. Grouping by relation makes the common edit local, and makes a
/// relation that permits nothing visible as an empty list rather than as an
/// absence spread across five entries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relation {
    /// What the subject is to the resource — `viewer`, `editor`, `owner`.
    pub name: RelationName,

    /// The operations this relation permits.
    ///
    /// Never empty: see the validation rules. A relation that permits nothing
    /// grants nothing, and the platform refuses it rather than reconciling a
    /// declaration that cannot have been meant.
    pub permits: Vec<OperationKind>,
}

impl Relation {
    /// Whether this relation permits the operation.
    #[must_use]
    pub fn permits(&self, operation: OperationKind) -> bool {
        self.permits.contains(&operation)
    }
}
