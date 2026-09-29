//! The shape rules an application's own resources satisfy on their own.
//!
//! The cross-application conflict -- a name another application already
//! published -- needs the whole catalogue, and stays in
//! `fabric-client-model`.
use super::unique;
use crate::errors::{invalid, ContractError};
use crate::ApplicationResource;

/// Checks one application's resources are internally coherent: unique
/// names, non-empty and duplicate-free operations, and duplicate-free
/// queryable fields that include the key field whenever the list restricts
/// anything.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] naming the resource at fault.
pub fn validate_resources(resources: &[ApplicationResource]) -> Result<(), ContractError> {
    unique(resources.iter().map(|r| r.name.as_str()), "resource")?;

    for resource in resources {
        if resource.operations.is_empty() {
            return Err(invalid(format!(
                "Resource '{}' must permit at least one operation",
                resource.name
            )));
        }
        unique(
            resource.operations.iter().map(|op| op.as_str()),
            "resource operation",
        )?;
        unique(
            resource
                .queryable_fields
                .iter()
                .map(fabric_runtime_publication::FieldName::as_str),
            "queryable field",
        )?;
        if !resource.queryable_fields.is_empty()
            && !resource
                .queryable_fields
                .iter()
                .any(|field| field == &resource.key_field)
        {
            return Err(invalid(format!(
                "Resource '{}' must include its key field '{}' among its queryable fields",
                resource.name, resource.key_field
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabric_runtime_publication::FieldName;

    fn resource(name: &str) -> ApplicationResource {
        ApplicationResource {
            name: fabric_core::LogicalResourceName::try_new(name).unwrap(),
            data_source: fabric_core::LogicalDataSourceName::try_new("primary").unwrap(),
            collection: fabric_runtime_publication::CollectionName::try_new(name).unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        }
    }

    #[test]
    fn duplicate_names_are_refused() {
        let error = validate_resources(&[resource("customers"), resource("customers")]).unwrap_err();

        assert_eq!(error.to_string(), "Duplicate resource: customers");
    }

    #[test]
    fn a_resource_with_no_operations_is_refused() {
        let error = validate_resources(&[ApplicationResource {
            operations: vec![],
            ..resource("customers")
        }])
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Resource 'customers' must permit at least one operation"
        );
    }

    #[test]
    fn a_key_field_missing_from_a_restricting_list_is_refused() {
        let error = validate_resources(&[ApplicationResource {
            queryable_fields: vec![FieldName::try_new("name").unwrap()],
            ..resource("customers")
        }])
        .unwrap_err();

        assert!(error.to_string().contains("key field 'id'"), "{error}");
    }

    #[test]
    fn an_unrestricted_resource_needs_no_key_field_listed() {
        validate_resources(&[resource("customers")]).unwrap();
    }
}
