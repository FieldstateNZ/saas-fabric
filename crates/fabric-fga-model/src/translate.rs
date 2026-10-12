//! `spec.authorization` to an authorization model.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{AuthorizationConfiguration, DesiredStateError, ResourceAuthorization};
use fabric_core::authorization_names::PRINCIPAL_TYPE;
use fabric_core::{CollectionObject, OperationKind};

use crate::model::{
    AuthorizationModel, Empty, Metadata, RelatedType, RelationMetadata, TypeDefinition, Userset,
};
use rewrite::{first_relation, permitted_by};

/// The authorization model `configuration` declares, or `None` when it
/// declares nothing.
///
/// `None` is not an empty model. A client that declares no authorization is
/// not managed by the platform (ADR 0013), and a principal-only model would be
/// something a later writer could mistake for one to activate.
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
/// The [`DesiredStateError`] validation reports. The configuration is
/// validated again here, because its fields are public and a value built in
/// code never passed through document parsing. A duplicate type or relation
/// is refused, never overwritten.
pub fn authorization_model(
    configuration: &AuthorizationConfiguration,
) -> Result<Option<AuthorizationModel>, DesiredStateError> {
    configuration.validate()?;

    if configuration.is_empty() {
        return Ok(None);
    }

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

        types.push(object_type(declared.resource.to_string(), declared, false)?);
        types.push(object_type(collection.type_name(), declared, true)?);
    }

    let mut names = BTreeSet::new();
    if let Some(duplicate) = types
        .iter()
        .find(|definition| !names.insert(definition.name.as_str()))
    {
        return Err(DesiredStateError::Duplicate {
            field: "spec.authorization.resources[].resource",
            value: duplicate.name.clone(),
        });
    }

    Ok(Some(AuthorizationModel::new(types)))
}

/// One object type: the declared relations, held directly, and the computed
/// relation of each operation asked about this kind of object.
fn object_type(
    name: String,
    declared: &ResourceAuthorization,
    collection: bool,
) -> Result<TypeDefinition, DesiredStateError> {
    let mut relations = BTreeMap::new();
    let mut direct = BTreeMap::new();
    let first = first_relation(&declared.relations)?;

    for relation in &declared.relations {
        insert(
            &mut relations,
            relation.name.to_string(),
            Userset::Direct(Empty {}),
        )?;
        direct.insert(
            relation.name.to_string(),
            RelationMetadata {
                directly_related_user_types: vec![RelatedType { name: PRINCIPAL_TYPE }],
            },
        );
    }

    for operation in OperationKind::ALL {
        if operation.is_collection_operation() == collection {
            insert(
                &mut relations,
                operation.computed_relation().to_owned(),
                permitted_by(&declared.relations, first, operation),
            )?;
        }
    }

    Ok(TypeDefinition {
        name,
        relations,
        metadata: Some(Metadata { relations: direct }),
    })
}

/// Adds a relation, refusing one that is already there.
fn insert(
    relations: &mut BTreeMap<String, Userset>,
    name: String,
    userset: Userset,
) -> Result<(), DesiredStateError> {
    if relations.contains_key(&name) {
        return Err(DesiredStateError::Duplicate {
            field: "spec.authorization.resources[].relations",
            value: name,
        });
    }

    relations.insert(name, userset);
    Ok(())
}

mod rewrite;

#[cfg(test)]
mod translate_tests;
