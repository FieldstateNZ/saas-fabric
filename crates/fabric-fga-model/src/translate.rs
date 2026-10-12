//! `spec.authorization` to an authorization model.

use std::collections::BTreeMap;

use fabric_client_model::{AuthorizationConfiguration, DesiredStateError, Relation, ResourceAuthorization};
use fabric_core::{CollectionObject, OperationKind};

use crate::model::{
    AuthorizationModel, Empty, Metadata, RelatedType, RelationMetadata, TypeDefinition, Userset,
    PRINCIPAL_TYPE,
};

/// The authorization model `configuration` declares.
///
/// Each declared resource becomes two types:
///
/// - its **record** type, `customers`, with every declared relation held
///   directly and `can_read`, `can_update` and `can_delete`;
/// - its **collection** type, `customers_collection`, with the same direct
///   relations and `can_list` and `can_create` only.
///
/// Each `can_<operation>` is held by whoever holds any relation that permits
/// the operation. Where no relation does, it is still defined, and nobody
/// holds it, so a Check answers a denial rather than a model error.
///
/// # Errors
///
/// The [`DesiredStateError`] validation reports, when the configuration is one
/// the client model would refuse. Every rule a model depends on — a
/// nameable collection type, no collision between types, no relation named
/// like a computed one — is a validation rule, so a document that validates
/// always translates.
pub fn authorization_model(
    configuration: &AuthorizationConfiguration,
) -> Result<AuthorizationModel, DesiredStateError> {
    configuration.validate()?;

    let mut types = vec![TypeDefinition {
        name: PRINCIPAL_TYPE.to_owned(),
        relations: BTreeMap::new(),
        metadata: None,
    }];

    for declared in &configuration.resources {
        let collection = CollectionObject::try_new(declared.resource.clone()).map_err(|error| {
            DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].resource",
                detail: error.to_string(),
            }
        })?;

        types.push(object_type(declared.resource.to_string(), declared, false));
        types.push(object_type(collection.type_name(), declared, true));
    }

    Ok(AuthorizationModel::new(types))
}

/// One object type: the declared relations, held directly, and the computed
/// relation of each operation asked about this kind of object.
fn object_type(name: String, declared: &ResourceAuthorization, collection: bool) -> TypeDefinition {
    let mut relations = BTreeMap::new();
    let mut direct = BTreeMap::new();

    for relation in &declared.relations {
        relations.insert(relation.name.to_string(), Userset::Direct(Empty {}));
        direct.insert(
            relation.name.to_string(),
            RelationMetadata {
                directly_related_user_types: vec![RelatedType { name: PRINCIPAL_TYPE }],
            },
        );
    }

    for operation in OperationKind::ALL {
        if operation.is_collection_operation() == collection {
            relations.insert(
                operation.computed_relation().to_owned(),
                permitted_by(&declared.relations, operation),
            );
        }
    }

    TypeDefinition {
        name,
        relations,
        metadata: Some(Metadata { relations: direct }),
    }
}

/// Whoever holds a relation that permits `operation`.
///
/// Validation guarantees at least one relation. When none permits the
/// operation, the first relation minus itself: a relation nobody can hold,
/// and which no tuple can be written for, because it is not direct.
fn permitted_by(relations: &[Relation], operation: OperationKind) -> Userset {
    let mut granting: Vec<Userset> = relations
        .iter()
        .filter(|relation| relation.permits(operation))
        .map(computed)
        .collect();

    if granting.len() > 1 {
        return Userset::Union { child: granting };
    }

    if let Some(only) = granting.pop() {
        return only;
    }

    // An empty union only for a resource with no relations, which validation
    // refuses; the service would refuse the model too.
    relations
        .first()
        .map_or(Userset::Union { child: Vec::new() }, |first| {
            Userset::Difference {
                base: Box::new(computed(first)),
                subtract: Box::new(computed(first)),
            }
        })
}

/// Whoever holds `relation` on the same object.
fn computed(relation: &Relation) -> Userset {
    Userset::Computed {
        relation: relation.name.to_string(),
    }
}

#[cfg(test)]
mod translate_tests;
