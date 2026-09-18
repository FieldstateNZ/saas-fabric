//! Deriving the runtime catalogue from every application's newest release.

use std::collections::BTreeMap;

use fabric_core::LogicalResourceName;
use fabric_runtime_publication::{CatalogDocument, ResourceDefinitionDocument};

use super::Catalogue;
use crate::ClientId;

/// One resource in the derived runtime catalogue: its definition, and which
/// application release it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedResource {
    /// The name callers address this resource by.
    pub name: LogicalResourceName,
    /// The definition the runtime would be given.
    pub definition: ResourceDefinitionDocument,
    /// The application whose release declares this resource.
    pub application: ClientId,
    /// The version of that application's release.
    pub version: u32,
}

/// The runtime catalogue, derived fresh from the product catalogue rather
/// than stored anywhere (ADR 0023 part 3). Nothing is published to the
/// runtime from this slice; part 4 builds the publisher.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DerivedCatalogue {
    /// Every derived resource, sorted by name.
    pub resources: Vec<DerivedResource>,
}

impl DerivedCatalogue {
    /// Converts the derived catalogue into the runtime publisher's own
    /// `catalog.json` shape, dropping which application and version each
    /// resource came from -- the wire carries a definition, not its
    /// provenance.
    #[must_use]
    pub fn into_document(self) -> CatalogDocument {
        let resources = self
            .resources
            .into_iter()
            .map(|resource| (resource.name, resource.definition))
            .collect::<BTreeMap<_, _>>();
        CatalogDocument::new(resources)
    }
}

/// Two different applications declare one resource name.
///
/// `PublishApplication` already refuses this before either release can
/// exist (see `catalogue::validation::check_cross_application_conflicts`),
/// so this is reachable only when the catalogue was edited by hand outside
/// the console. Reported, never guessed: nothing here picks a winner.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("resource '{resource}' is declared by both '{}' and '{}'", applications.0, applications.1)]
pub struct CatalogueConflict {
    /// The resource name two applications both declare.
    pub resource: LogicalResourceName,
    /// The two applications that declare it.
    pub applications: (ClientId, ClientId),
}

impl Catalogue {
    /// Derives the runtime catalogue: for every application, the newest
    /// published release's resources, superseding an older release's
    /// definition of the same name (ADR 0023 part 3). A draft never reaches
    /// the runtime -- only published releases are read.
    ///
    /// Pure and cheap enough to call on every read: nothing is cached, so a
    /// console showing this panel always sees what publishing again would
    /// now produce.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogueConflict`] if two different applications declare
    /// one resource name in their newest release.
    pub fn runtime_catalogue(&self) -> Result<DerivedCatalogue, CatalogueConflict> {
        let mut resources: BTreeMap<LogicalResourceName, DerivedResource> = BTreeMap::new();

        for app in &self.applications {
            // Releases are pushed in increasing `version` order
            // (`Catalogue::validate`), so the last one is the newest.
            let Some(release) = app.releases.last() else {
                continue;
            };

            for resource in release.definition.resources.clone() {
                let (name, definition) = resource.into_definition();

                if let Some(existing) = resources.get(&name) {
                    if existing.application != app.id {
                        return Err(CatalogueConflict {
                            resource: name,
                            applications: (existing.application.clone(), app.id.clone()),
                        });
                    }
                }

                resources.insert(
                    name.clone(),
                    DerivedResource {
                        name,
                        definition,
                        application: app.id.clone(),
                        version: release.version,
                    },
                );
            }
        }

        Ok(DerivedCatalogue {
            resources: resources.into_values().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{Application, ApplicationDefinition, ApplicationRelease, ApplicationResource};

    fn resource(name: &str, queryable: &[&str]) -> ApplicationResource {
        ApplicationResource {
            name: LogicalResourceName::try_new(name).unwrap(),
            data_source: fabric_core::LogicalDataSourceName::try_new("primary").unwrap(),
            collection: fabric_runtime_publication::CollectionName::try_new(name).unwrap(),
            key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: queryable
                .iter()
                .map(|field| fabric_runtime_publication::FieldName::try_new(*field).unwrap())
                .collect(),
        }
    }

    fn app(id: &str, releases: Vec<(u32, Vec<ApplicationResource>)>) -> Application {
        let releases = releases
            .into_iter()
            .map(|(version, resources)| ApplicationRelease {
                version,
                note: String::new(),
                published_at: 0,
                definition: ApplicationDefinition {
                    resources,
                    ..ApplicationDefinition::default()
                },
            })
            .collect();
        Application {
            id: ClientId::try_new(id).unwrap(),
            draft: ApplicationDefinition::default(),
            releases,
        }
    }

    #[test]
    fn an_empty_catalogue_derives_no_resources() {
        let derived = Catalogue::default().runtime_catalogue().unwrap();

        assert!(derived.resources.is_empty());
    }

    #[test]
    fn a_draft_only_change_never_reaches_the_derived_catalogue() {
        let mut catalogue = Catalogue {
            applications: vec![app("workspec", vec![(1, vec![resource("customers", &[])])])],
            ..Catalogue::default()
        };
        catalogue.applications[0].draft = ApplicationDefinition {
            resources: vec![resource("orders", &[])],
            ..ApplicationDefinition::default()
        };

        let derived = catalogue.runtime_catalogue().unwrap();

        assert_eq!(derived.resources.len(), 1);
        assert_eq!(derived.resources[0].name.as_str(), "customers");
    }

    #[test]
    fn the_newest_release_supersedes_an_older_definition_of_the_same_name() {
        let catalogue = Catalogue {
            applications: vec![app(
                "workspec",
                vec![
                    (1, vec![resource("customers", &[])]),
                    (2, vec![resource("customers", &["id", "name"])]),
                ],
            )],
            ..Catalogue::default()
        };

        let derived = catalogue.runtime_catalogue().unwrap();

        assert_eq!(derived.resources.len(), 1);
        assert_eq!(derived.resources[0].version, 2);
        assert_eq!(derived.resources[0].definition.queryable_fields.len(), 2);
    }

    #[test]
    fn two_applications_declaring_one_name_is_a_conflict() {
        let catalogue = Catalogue {
            applications: vec![
                app("workspec", vec![(1, vec![resource("customers", &[])])]),
                app("other", vec![(1, vec![resource("customers", &[])])]),
            ],
            ..Catalogue::default()
        };

        let error = catalogue.runtime_catalogue().unwrap_err();

        assert_eq!(error.resource.as_str(), "customers");
        let names = [error.applications.0.as_str(), error.applications.1.as_str()];
        assert!(names.contains(&"workspec"), "{names:?}");
        assert!(names.contains(&"other"), "{names:?}");
    }

    #[test]
    fn deriving_into_a_catalog_document_carries_every_resources_definition() {
        let catalogue = Catalogue {
            applications: vec![app("workspec", vec![(1, vec![resource("customers", &["id"])])])],
            ..Catalogue::default()
        };

        let document = catalogue.runtime_catalogue().unwrap().into_document();

        let name = LogicalResourceName::try_new("customers").unwrap();
        assert!(document.get(&name).is_some());
        assert_eq!(document.len(), 1);
    }
}
