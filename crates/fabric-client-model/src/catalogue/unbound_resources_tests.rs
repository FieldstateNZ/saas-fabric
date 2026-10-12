//! `DerivedCatalogue::unbound_for` (G8).

use std::collections::BTreeSet;

use fabric_core::LogicalDataSourceName;

use crate::catalogue::{
    Application, ApplicationDefinition, ApplicationRelease, ApplicationResource, Catalogue,
};
use crate::ClientId;

fn resource(name: &str, data_source: &str) -> ApplicationResource {
    ApplicationResource {
        name: fabric_core::LogicalResourceName::try_new(name).unwrap(),
        data_source: LogicalDataSourceName::try_new(data_source).unwrap(),
        collection: fabric_runtime_publication::CollectionName::try_new(name).unwrap(),
        key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
        operations: vec![fabric_core::OperationKind::Read],
        queryable_fields: Vec::new(),
    }
}

fn app(id: &str, version: u32, resources: Vec<ApplicationResource>) -> Application {
    Application {
        id: ClientId::try_new(id).unwrap(),
        draft: ApplicationDefinition::default(),
        releases: vec![ApplicationRelease {
            version,
            note: String::new(),
            published_at: 0,
            definition: ApplicationDefinition {
                resources,
                ..ApplicationDefinition::default()
            },
        }],
    }
}

fn bound(names: &[&str]) -> BTreeSet<LogicalDataSourceName> {
    names
        .iter()
        .map(|name| LogicalDataSourceName::try_new(*name).unwrap())
        .collect()
}

fn catalogue() -> Catalogue {
    Catalogue {
        applications: vec![
            app(
                "workspec",
                2,
                vec![
                    resource("customers", "primary"),
                    resource("orders", "primary"),
                    resource("audit-events", "audit"),
                ],
            ),
            app(
                "ledger",
                1,
                vec![resource("journals", "audit"), resource("rates", "reference")],
            ),
        ],
        ..Catalogue::default()
    }
}

#[test]
fn each_logical_data_source_the_tenant_does_not_bind_lists_every_resource_on_it_with_its_release() {
    let derived = catalogue().runtime_catalogue().unwrap();

    let unbound = derived.unbound_for(&bound(&["primary"]));

    let summary: Vec<(String, Vec<String>)> = unbound
        .iter()
        .map(|entry| {
            let resources = entry
                .resources
                .iter()
                .map(|resource| format!("{}@{}:{}", resource.name, resource.application, resource.version))
                .collect();
            (entry.logical.to_string(), resources)
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "audit".to_owned(),
                vec![
                    "audit-events@workspec:2".to_owned(),
                    "journals@ledger:1".to_owned()
                ],
            ),
            ("reference".to_owned(), vec!["rates@ledger:1".to_owned()]),
        ]
    );
}

#[test]
fn a_tenant_that_binds_every_named_logical_data_source_has_no_unbound_pair() {
    let derived = catalogue().runtime_catalogue().unwrap();

    assert!(derived
        .unbound_for(&bound(&["audit", "primary", "reference", "unused"]))
        .is_empty());
}

#[test]
fn a_tenant_with_no_placement_cannot_reach_any_resource() {
    let derived = catalogue().runtime_catalogue().unwrap();

    let logicals: Vec<String> = derived
        .unbound_for(&BTreeSet::new())
        .iter()
        .map(|entry| entry.logical.to_string())
        .collect();
    assert_eq!(logicals, ["audit", "primary", "reference"]);
}

#[test]
fn an_empty_catalogue_names_nothing_unbound() {
    let derived = Catalogue::default().runtime_catalogue().unwrap();

    assert!(derived.unbound_for(&BTreeSet::new()).is_empty());
}
