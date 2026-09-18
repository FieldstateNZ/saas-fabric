//! Applies operator commands while owning version numbers and audit attribution.
//!
//! In the 121–150 line band. The reason is `apply` itself: one `match` over
//! every [`CatalogueCommand`] variant, because that match is exactly the
//! place a new command is wired in, and splitting each arm into its own
//! function would still leave this file naming all six and stitching their
//! results into one activity record — the coordination, not the individual
//! arms, is what makes this one function.
use super::validation::{check_cross_application_conflicts, invalid, text};
use super::{
    Application, ApplicationDefinition, ApplicationRelease, Catalogue, CatalogueCommand, ProductActivity,
};
use crate::DesiredStateError;
impl Catalogue {
    /// Applies a command to a copy; errors leave the original snapshot unchanged.
    /// # Errors
    /// Rejects unknown resources, duplicate keys and invalid definitions.
    pub fn apply(
        &self,
        command: CatalogueCommand,
        operator: &str,
        at: u64,
    ) -> Result<Self, DesiredStateError> {
        let mut next = self.clone();
        let (action, resource) = match command {
            CatalogueCommand::CreateApplication { id, name } => {
                if next.applications.iter().any(|a| a.id == id) {
                    return Err(invalid("Application already exists"));
                }
                // An application id becomes an OIDC client id in every realm
                // it is assigned to (`with_application_identity`). `ClientId`
                // admits a leading digit; `OidcClientId` does not — so
                // without this, `1app` publishes cleanly and then fails
                // every assignment instead of failing here, once, where the
                // id is chosen.
                crate::OidcClientId::try_new(id.as_str()).map_err(|error| {
                    invalid(format!("Application id is not a valid OIDC client id: {error}"))
                })?;
                // Keycloak already has a client under each of these ids in
                // every realm it manages — see
                // [`reserved_client_ids::RESERVED_APPLICATION_IDS`]. A
                // deployment-specific id (the console's own, or a converged
                // Keycloak's admin client) is a separate, composition-root
                // concern this catalogue knows nothing about; see
                // `ClientService::check_application_id_available`.
                if crate::reserved_client_ids::RESERVED_APPLICATION_IDS.contains(&id.as_str()) {
                    return Err(invalid(format!(
                        "Application id '{id}' is reserved by Keycloak's own realm-managed client"
                    )));
                }
                let resource = id.to_string();
                next.applications.push(Application {
                    id,
                    draft: ApplicationDefinition {
                        name,
                        ..ApplicationDefinition::default()
                    },
                    releases: vec![],
                });
                ("Application created", resource)
            }
            CatalogueCommand::SaveApplication { id, definition } => {
                let app = next
                    .applications
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or_else(|| invalid("Application does not exist"))?;
                app.draft = definition;
                ("Application draft saved", id.to_string())
            }
            CatalogueCommand::PublishApplication { id, note } => {
                text(&note, "Release note", true, 2048)?;
                let draft = next
                    .applications
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| invalid("Application does not exist"))?
                    .draft
                    .clone();
                draft.validate(true)?;
                // Publish-time only, and never on a save: this is the one
                // rule that needs the rest of the catalogue, not just this
                // application's own draft, so it lives here rather than in
                // `ApplicationDefinition::validate` (ADR 0023 part 3).
                check_cross_application_conflicts(&id, &draft.resources, &next.applications)?;
                let app = next
                    .applications
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or_else(|| invalid("Application does not exist"))?;
                if app.releases.last().is_some_and(|r| r.definition == app.draft) {
                    return Err(invalid("This definition is already published"));
                }
                let version = app
                    .releases
                    .last()
                    .map_or(0, |r| r.version)
                    .checked_add(1)
                    .ok_or_else(|| invalid("Version limit reached"))?;
                app.releases.push(ApplicationRelease {
                    version,
                    note,
                    published_at: at,
                    definition: app.draft.clone(),
                });
                ("Application definition published", id.to_string())
            }
            CatalogueCommand::SaveDefinition { fields } => {
                next.client_fields = fields;
                next.definition_version = next
                    .definition_version
                    .checked_add(1)
                    .ok_or_else(|| invalid("Version limit reached"))?;
                ("Client definition updated", "client-definition".into())
            }
            CatalogueCommand::SaveSettings { settings } => {
                next.settings = settings;
                ("Settings updated", "settings".into())
            }
            CatalogueCommand::SaveEnvironment { environment } => {
                let resource = environment.id.to_string();
                if let Some(existing) = next.environments.iter_mut().find(|e| e.id == environment.id) {
                    *existing = environment;
                } else {
                    next.environments.push(environment);
                }
                ("Environment registered", resource)
            }
        };
        next.activity.push(ProductActivity {
            at,
            operator: operator.into(),
            action: action.into(),
            resource,
        });
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientId;

    #[test]
    fn an_id_a_leading_digit_makes_invalid_as_an_oidc_client_id_is_refused() {
        let error = Catalogue::default()
            .apply(
                CatalogueCommand::CreateApplication {
                    id: ClientId::try_new("1app").unwrap(),
                    name: "Numbers First".into(),
                },
                "brett@example.com",
                1,
            )
            .unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("OIDC client id")),
            "{error}"
        );
    }

    #[test]
    fn a_realm_managed_keycloak_client_id_is_refused() {
        let error = Catalogue::default()
            .apply(
                CatalogueCommand::CreateApplication {
                    id: ClientId::try_new("account-console").unwrap(),
                    name: "Shadowing Keycloak".into(),
                },
                "brett@example.com",
                1,
            )
            .unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("reserved")),
            "{error}"
        );
    }

    #[test]
    fn publishing_a_resource_name_another_application_already_published_is_refused() {
        let resource = crate::catalogue::ApplicationResource {
            name: fabric_core::LogicalResourceName::try_new("customers").unwrap(),
            data_source: fabric_core::LogicalDataSourceName::try_new("primary").unwrap(),
            collection: fabric_runtime_publication::CollectionName::try_new("customers").unwrap(),
            key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        };
        let plan = crate::catalogue::ApplicationPlan {
            id: ClientId::try_new("standard").unwrap(),
            name: "Standard".into(),
            description: String::new(),
            features: vec![],
            configuration: crate::catalogue::ConfigurationValues::default(),
        };
        let mut catalogue = Catalogue {
            applications: vec![Application {
                id: ClientId::try_new("other").unwrap(),
                draft: ApplicationDefinition {
                    name: "Other".into(),
                    ..ApplicationDefinition::default()
                },
                releases: vec![super::ApplicationRelease {
                    version: 1,
                    note: String::new(),
                    published_at: 0,
                    definition: ApplicationDefinition {
                        name: "Other".into(),
                        resources: vec![resource.clone()],
                        plans: vec![plan],
                        ..ApplicationDefinition::default()
                    },
                }],
            }],
            ..Catalogue::default()
        };
        catalogue = catalogue
            .apply(
                CatalogueCommand::CreateApplication {
                    id: ClientId::try_new("workspec").unwrap(),
                    name: "WorkSpec".into(),
                },
                "brett@example.com",
                1,
            )
            .unwrap();
        catalogue.applications[1].draft = ApplicationDefinition {
            name: "WorkSpec".into(),
            resources: vec![resource],
            plans: vec![crate::catalogue::ApplicationPlan {
                id: ClientId::try_new("standard").unwrap(),
                name: "Standard".into(),
                description: String::new(),
                features: vec![],
                configuration: crate::catalogue::ConfigurationValues::default(),
            }],
            ..ApplicationDefinition::default()
        };

        let error = catalogue
            .apply(
                CatalogueCommand::PublishApplication {
                    id: ClientId::try_new("workspec").unwrap(),
                    note: "Initial release".into(),
                },
                "brett@example.com",
                2,
            )
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("workspec"), "{message}");
        assert!(message.contains("other"), "{message}");
    }

    #[test]
    fn an_ordinary_application_id_is_created() {
        let catalogue = Catalogue::default()
            .apply(
                CatalogueCommand::CreateApplication {
                    id: ClientId::try_new("analytics").unwrap(),
                    name: "Analytics".into(),
                },
                "brett@example.com",
                1,
            )
            .unwrap();

        assert_eq!(catalogue.applications[0].id.as_str(), "analytics");
    }
}
