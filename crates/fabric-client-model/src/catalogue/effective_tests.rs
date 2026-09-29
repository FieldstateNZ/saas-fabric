//! Declared content is in effect in every reader (ADR 0026 section 8).
use super::described_tests::{
    authored, catalogue, definition, described, descriptor, id, resolution, FIRST, REPORTS,
};
use super::*;

fn with_reports() -> ApplicationDefinition {
    definition(vec![described("reports", resolution("1.4.0", FIRST))])
}

#[test]
fn the_content_in_effect_is_authored_then_declared_in_component_order() {
    let mut definition = definition(vec![
        authored("web", ComponentKind::Container),
        described("reports", resolution("1.4.0", FIRST)),
    ]);
    definition.fields = vec![ConfigurationField {
        key: "region".into(),
        ..descriptor("1.4.0", super::described_tests::TEAM, "[]")
            .spec()
            .fields[0]
            .clone()
    }];

    let keys: Vec<String> = definition
        .effective_fields()
        .into_iter()
        .map(|field| field.key)
        .collect();
    let resources: Vec<String> = definition
        .effective_resources()
        .into_iter()
        .map(|resource| resource.name.to_string())
        .collect();

    assert_eq!(keys, ["region", "team"]);
    assert_eq!(resources, ["reports"]);
    assert!(definition.fields.len() == 1 && definition.resources.is_empty());
}

#[test]
fn a_declared_resource_reaches_the_runtime_catalogue() {
    let catalogue = catalogue(definition(vec![]), vec![with_reports()]);

    let derived = catalogue.runtime_catalogue().unwrap();

    assert_eq!(derived.resources.len(), 1);
    assert_eq!(derived.resources[0].name.as_str(), "reports");
    assert_eq!(derived.resources[0].application.as_str(), "analytics");
}

fn request(configuration: &[(&str, &str)]) -> ClientProductRequest {
    ClientProductRequest {
        display_name: "Acme".into(),
        hosts: vec![],
        legal_name: "Acme Ltd".into(),
        region: "NZ".into(),
        timezone: "Pacific/Auckland".into(),
        configuration: ConfigurationValues::default(),
        applications: vec![AssignmentRequest {
            application_id: id("analytics"),
            version: 1,
            plan_id: id("standard"),
            configuration: configuration
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        }],
    }
}

#[test]
fn a_declared_required_field_is_enforced_for_every_client() {
    let catalogue = catalogue(definition(vec![]), vec![with_reports()]);

    let missing = catalogue.resolve(&request(&[]), &[]).unwrap_err();
    let product = catalogue.resolve(&request(&[("team", "Finance")]), &[]).unwrap();

    assert_eq!(missing.to_string(), "catalogue: Team is required");
    assert_eq!(product.applications[0].configuration["team"], "Finance");
}

/// `other`, published with `released`, beside `analytics`, whose draft is
/// `draft`.
fn beside_other(draft: ApplicationDefinition, released: ApplicationDefinition) -> Catalogue {
    let mut catalogue = catalogue(draft, vec![]);
    catalogue.applications.push(Application {
        id: id("other"),
        draft: definition(vec![]),
        releases: vec![ApplicationRelease {
            version: 1,
            note: String::new(),
            published_at: 0,
            definition: released,
        }],
    });
    catalogue
}

fn publish(catalogue: &Catalogue) -> String {
    catalogue
        .apply(
            CatalogueCommand::PublishApplication {
                id: id("analytics"),
                note: "Initial release".into(),
            },
            "brett@example.com",
            1,
        )
        .unwrap_err()
        .to_string()
}

#[test]
fn a_declared_resource_colliding_with_another_applications_published_one_refuses_the_publish() {
    let mut authored_elsewhere = definition(vec![]);
    authored_elsewhere.resources = descriptor("1.4.0", "[]", REPORTS).spec().resources.clone();

    let error = publish(&beside_other(with_reports(), authored_elsewhere));

    assert_eq!(
        error,
        "catalogue: Resource 'reports' is already declared by application 'other' and cannot also belong to 'analytics'"
    );
}

#[test]
fn an_authored_resource_colliding_with_another_applications_declared_one_refuses_the_publish() {
    let mut draft = definition(vec![]);
    draft.resources = descriptor("1.4.0", "[]", REPORTS).spec().resources.clone();

    let error = publish(&beside_other(draft, with_reports()));

    assert!(
        error.contains("'reports' is already declared by application 'other'"),
        "{error}"
    );
}
