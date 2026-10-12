//! How each `can_<operation>` is computed from the declared relations.

use fabric_client_model::{DesiredStateError, Relation};
use fabric_core::OperationKind;

use crate::model::Userset;

/// The relation "nobody" is built from: the least by name, so the model does
/// not depend on the order relations are declared in.
pub(super) fn first_relation(relations: &[Relation]) -> Result<&Relation, DesiredStateError> {
    relations
        .iter()
        .min_by(|left, right| left.name.cmp(&right.name))
        .ok_or_else(|| DesiredStateError::InvalidField {
            field: "spec.authorization.resources[].relations",
            detail: "a resource with no relations has no model".to_owned(),
        })
}

/// Whoever holds a relation that permits `operation`.
///
/// One contributor is that relation itself, because the service refuses a
/// union of one. Several are a union. None is `first` minus itself: a relation
/// nobody can hold, and which no tuple can be written for, because it is not
/// direct.
pub(super) fn permitted_by(relations: &[Relation], first: &Relation, operation: OperationKind) -> Userset {
    let mut granting: Vec<Userset> = relations
        .iter()
        .filter(|relation| relation.permits(operation))
        .map(computed)
        .collect();

    if granting.len() > 1 {
        return Userset::Union { child: granting };
    }

    granting.pop().unwrap_or_else(|| Userset::Difference {
        base: Box::new(computed(first)),
        subtract: Box::new(computed(first)),
    })
}

/// Whoever holds `relation` on the same object.
fn computed(relation: &Relation) -> Userset {
    Userset::Computed {
        relation: relation.name.to_string(),
    }
}
