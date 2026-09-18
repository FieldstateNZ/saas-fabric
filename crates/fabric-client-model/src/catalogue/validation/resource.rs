//! A resource is checked twice: shape rules any application can satisfy on
//! its own, and the cross-application conflict only the whole catalogue can
//! see.

use super::{invalid, unique};
use crate::catalogue::{Application, ApplicationDefinition, ApplicationResource};
use crate::{ClientId, DesiredStateError};

impl ApplicationDefinition {
    /// Checks this application's own resources are internally coherent:
    /// unique names, non-empty and duplicate-free operations, and
    /// duplicate-free queryable fields that include the key field whenever
    /// the list restricts anything.
    ///
    /// Run on every save, the same as the other sections' own checks --
    /// unlike [`check_cross_application_conflicts`], nothing here needs to
    /// see another application.
    pub(super) fn validate_resources(&self) -> Result<(), DesiredStateError> {
        unique(self.resources.iter().map(|r| r.name.as_str()), "resource")?;

        for resource in &self.resources {
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
}

/// Refuses publishing a definition whose resources would collide with a
/// name a *different* application has already **published**, in any of its
/// releases.
///
/// Publication is what takes a name. A draft never does: a draft is
/// unpublished, may itself be unpublishable, and counting it would let one
/// operator's half-written application block the rightful owner's next
/// release -- the exact case the first cut of this check got wrong. The
/// draft's own collision is caught when *it* is published, by this same
/// check running the other way round.
///
/// The only reason [`Catalogue::runtime_catalogue`](crate::catalogue::Catalogue::runtime_catalogue)
/// can ever find a conflict is a hand edit outside the console: this check
/// is what keeps Fabric's own writes from ever producing one, by refusing
/// the *second* application's publish rather than guessing which of the two
/// should win.
///
/// # Errors
///
/// Returns [`DesiredStateError::InvalidField`] naming the resource and both
/// applications.
pub(in crate::catalogue) fn check_cross_application_conflicts(
    publishing: &ClientId,
    resources: &[ApplicationResource],
    applications: &[Application],
) -> Result<(), DesiredStateError> {
    for resource in resources {
        for other in applications.iter().filter(|app| &app.id != publishing) {
            let published_by_other = other.releases.iter().any(|release| {
                release
                    .definition
                    .resources
                    .iter()
                    .any(|r| r.name == resource.name)
            });
            if published_by_other {
                return Err(invalid(format!(
                    "Resource '{}' is already declared by application '{}' and cannot also belong to '{}'",
                    resource.name, other.id, publishing
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::ApplicationRelease;

    fn resource(name: &str) -> ApplicationResource {
        ApplicationResource {
            name: fabric_core::LogicalResourceName::try_new(name).unwrap(),
            data_source: fabric_core::LogicalDataSourceName::try_new("primary").unwrap(),
            collection: fabric_runtime_publication::CollectionName::try_new(name).unwrap(),
            key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        }
    }

    #[test]
    fn duplicate_resource_names_within_one_application_are_refused() {
        let definition = ApplicationDefinition {
            resources: vec![resource("customers"), resource("customers")],
            ..ApplicationDefinition::default()
        };

        let error = definition.validate_resources().unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("Duplicate resource")),
            "{error}"
        );
    }

    #[test]
    fn a_resource_with_no_operations_is_refused() {
        let definition = ApplicationDefinition {
            resources: vec![ApplicationResource {
                operations: vec![],
                ..resource("customers")
            }],
            ..ApplicationDefinition::default()
        };

        let error = definition.validate_resources().unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("at least one operation")),
            "{error}"
        );
    }

    #[test]
    fn a_key_field_missing_from_a_nonempty_queryable_list_is_refused() {
        let definition = ApplicationDefinition {
            resources: vec![ApplicationResource {
                queryable_fields: vec![fabric_runtime_publication::FieldName::try_new("name").unwrap()],
                ..resource("customers")
            }],
            ..ApplicationDefinition::default()
        };

        let error = definition.validate_resources().unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("key field")),
            "{error}"
        );
    }

    #[test]
    fn a_repeated_operation_is_refused() {
        let definition = ApplicationDefinition {
            resources: vec![ApplicationResource {
                operations: vec![fabric_core::OperationKind::Read, fabric_core::OperationKind::Read],
                ..resource("customers")
            }],
            ..ApplicationDefinition::default()
        };

        let error = definition.validate_resources().unwrap_err();

        assert!(error.to_string().contains("resource operation"), "{error}");
    }

    #[test]
    fn a_repeated_queryable_field_is_refused() {
        let id = fabric_runtime_publication::FieldName::try_new("id").unwrap();
        let definition = ApplicationDefinition {
            resources: vec![ApplicationResource {
                queryable_fields: vec![id.clone(), id],
                ..resource("customers")
            }],
            ..ApplicationDefinition::default()
        };

        let error = definition.validate_resources().unwrap_err();

        assert!(error.to_string().contains("queryable field"), "{error}");
    }

    #[test]
    fn an_empty_queryable_list_does_not_require_the_key_field() {
        let definition = ApplicationDefinition {
            resources: vec![resource("customers")],
            ..ApplicationDefinition::default()
        };

        definition.validate_resources().unwrap();
    }

    fn published(id: &str, resources: Vec<ApplicationResource>) -> Application {
        Application {
            id: ClientId::try_new(id).unwrap(),
            draft: ApplicationDefinition::default(),
            releases: vec![ApplicationRelease {
                version: 1,
                note: String::new(),
                published_at: 0,
                definition: ApplicationDefinition {
                    resources,
                    ..ApplicationDefinition::default()
                },
            }],
        }
    }

    #[test]
    fn a_name_another_application_already_published_is_refused() {
        let workspec = ClientId::try_new("workspec").unwrap();
        let applications = vec![published("other", vec![resource("customers")])];

        let error = check_cross_application_conflicts(&workspec, &[resource("customers")], &applications)
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("workspec"), "{message}");
        assert!(message.contains("other"), "{message}");
    }

    #[test]
    fn a_name_only_this_application_declares_is_not_a_conflict_with_itself() {
        let workspec = ClientId::try_new("workspec").unwrap();
        let applications = vec![published("workspec", vec![resource("customers")])];

        check_cross_application_conflicts(&workspec, &[resource("customers")], &applications).unwrap();
    }

    #[test]
    fn a_name_declared_only_in_another_applications_draft_does_not_block_the_owner() {
        // The draft is unpublished and may never publish; counting it would
        // let it block the application that already owns the name.
        let workspec = ClientId::try_new("workspec").unwrap();
        let applications = vec![Application {
            id: ClientId::try_new("other").unwrap(),
            draft: ApplicationDefinition {
                resources: vec![resource("customers")],
                ..ApplicationDefinition::default()
            },
            releases: vec![],
        }];

        check_cross_application_conflicts(&workspec, &[resource("customers")], &applications).unwrap();
    }
}
