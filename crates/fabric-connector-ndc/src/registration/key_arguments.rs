//! Checking a mapping's key arguments against what the connector declares.

use fabric_connector::{CollectionName, FieldName};

use crate::config::ProcedureBinding;
use crate::schema_index::ArgumentKind;
use crate::{NdcConnectorConfig, SchemaIndex};

/// Refuses a key argument the procedure does not declare, declares as a
/// predicate, or whose source field the collection's own schema does not
/// have.
///
/// # Why a key is never a predicate
///
/// A key argument carries one scalar value read off an equality in the
/// predicate — `ndc-postgres`'s `key_id` is a plain `text`. Pointing
/// `key_arguments` at an argument the procedure declares as `predicate`-typed
/// would try to send that scalar somewhere only a full expression tree can
/// go, which is as broken as the mirror mistake `registration::procedure_arguments`
/// already refuses for `filter_argument`.
///
/// # Errors
///
/// A message naming the collection, the verb, the field, the argument, and
/// what is wrong with it.
pub(super) fn check_key_arguments(config: &NdcConnectorConfig, index: &SchemaIndex) -> Result<(), String> {
    for (collection, procedures) in &config.procedures {
        for (verb, binding) in procedures.all() {
            let Some(binding) = binding else { continue };

            check_binding(config, index, collection, verb, binding)?;
        }
    }

    Ok(())
}

/// Checks every key argument one mapping declares.
fn check_binding(
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
    collection: &str,
    verb: &str,
    binding: &ProcedureBinding,
) -> Result<(), String> {
    let procedure = &binding.procedure;
    let where_ = format!("connector {}: {collection}.{verb}", config.id);

    for (field, argument) in &binding.key_arguments {
        match index.procedure_argument(procedure, argument) {
            None => {
                return Err(format!(
                    "{where_} names `{argument}` as the key argument for field `{field}`, but procedure \
                     `{procedure}` does not declare it"
                ))
            }
            Some(ArgumentKind::Predicate) => {
                return Err(format!(
                    "{where_} names `{argument}` as the key argument for field `{field}`, but procedure \
                     `{procedure}` declares that argument as a predicate; a key is never a predicate"
                ))
            }
            Some(ArgumentKind::Value) => {}
        }

        check_field_known(index, collection, field, &where_)?;
    }

    Ok(())
}

/// Requires a key's source field to exist on the collection, when the
/// collection itself is one the schema declares.
///
/// A collection absent from the schema entirely is a different problem this
/// check does not own, so it says nothing rather than double-refusing. A
/// field that is missing on a collection the schema *does* declare would
/// otherwise only surface as "no equality found for that field" the first
/// time a write runs — indistinguishable from a caller who simply omitted the
/// filter.
fn check_field_known(index: &SchemaIndex, collection: &str, field: &str, where_: &str) -> Result<(), String> {
    let Ok(collection_name) = CollectionName::try_new(collection) else {
        return Ok(());
    };

    if index.neutral().collection(&collection_name).is_none() {
        return Ok(());
    }

    let known =
        FieldName::try_new(field).is_ok_and(|field_name| index.has_field(&collection_name, &field_name));

    if known {
        Ok(())
    } else {
        Err(format!(
            "{where_} names field `{field}` as a key, but the collection's schema has no such field"
        ))
    }
}
