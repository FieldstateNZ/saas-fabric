//! Every row of ADR 0026 section 7's selection table, the refusals beside
//! it, and a release freezing the resolution by copy.
use super::described_tests::{authored, catalogue, definition, described, id, resolution, FIRST, SECOND};
use super::*;
use serde_json::json;

fn select(
    catalogue: &Catalogue,
    component: &str,
    chosen: ComponentResolution,
) -> Result<Catalogue, ComponentSelectionError> {
    catalogue.select_component(&id("analytics"), &id(component), chosen, "brett@example.com", 9)
}

fn with(components: Vec<ApplicationComponent>) -> Catalogue {
    let mut draft = definition(components);
    draft.features = vec![ApplicationFeature {
        id: id("reporting"),
        name: "Reporting".into(),
        description: String::new(),
        implemented_by: draft
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect(),
    }];
    catalogue(draft, vec![])
}

#[test]
fn an_absent_id_creates_a_described_component_named_by_its_title() {
    let selected = select(&with(vec![]), "reports", resolution("1.4.0", FIRST)).unwrap();

    let component = &selected.applications[0].draft.components[0];
    assert_eq!(component.id.as_str(), "reports");
    assert_eq!(component.name, "Reports");
    assert_eq!(component.kind, ComponentKind::Described);
    assert_eq!(component.reference, "registry.example.com/acme/reports");
    assert_eq!(component.version, "1.4.0");
    assert!(!component.required);
    assert_eq!(component.policy, UpdatePolicy::Manual);
    assert_eq!(component.resolution, Some(resolution("1.4.0", FIRST)));
    let activity = selected.activity.last().unwrap();
    assert_eq!(activity.action, "Component version selected");
    assert_eq!(activity.resource, "analytics/reports");
    assert_eq!(
        (activity.at, activity.operator.as_str()),
        (9, "brett@example.com")
    );
}

#[test]
fn a_described_id_is_re_resolved_keeping_what_the_operator_decided() {
    let mut kept = described("reports", resolution("1.4.0", FIRST));
    kept.name = "Monthly reports".into();
    kept.required = true;
    kept.policy = UpdatePolicy::Automatic;

    let selected = select(&with(vec![kept]), "reports", resolution("1.5.0", SECOND)).unwrap();

    let component = &selected.applications[0].draft.components[0];
    assert_eq!(component.name, "Monthly reports");
    assert!(component.required);
    assert_eq!(component.policy, UpdatePolicy::Automatic);
    assert_eq!(component.version, "1.5.0");
    assert_eq!(component.resolution, Some(resolution("1.5.0", SECOND)));
}

#[test]
fn a_container_or_helm_id_is_converted_in_place() {
    for kind in [ComponentKind::Container, ComponentKind::Helm] {
        let catalogue = with(vec![authored("reports", kind)]);

        let selected = select(&catalogue, "reports", resolution("1.4.0", FIRST)).unwrap();

        let draft = &selected.applications[0].draft;
        let component = &draft.components[0];
        assert_eq!(component.kind, ComponentKind::Described, "{kind:?}");
        assert_eq!((component.name.as_str(), component.required), ("Authored", true));
        assert_eq!(component.policy, UpdatePolicy::Automatic);
        assert_eq!(component.reference, "registry.example.com/acme/reports");
        assert_eq!(draft.features, catalogue.applications[0].draft.features);
    }
}

#[test]
fn a_capability_id_is_refused() {
    let error = select(
        &with(vec![authored("reports", ComponentKind::Capability)]),
        "reports",
        resolution("1.4.0", FIRST),
    )
    .unwrap_err();

    assert!(matches!(error, ComponentSelectionError::Refused(_)), "{error}");
    assert!(error.to_string().contains("platform capability"), "{error}");
}

#[test]
fn the_descriptor_the_component_already_records_is_already_selected() {
    let catalogue = with(vec![described("reports", resolution("1.4.0", FIRST))]);
    let mut later = resolution("1.4.0", FIRST);
    later.resolved_at += 60;

    let error = select(&catalogue, "reports", later).unwrap_err();

    assert_eq!(
        error,
        ComponentSelectionError::AlreadySelected {
            application: id("analytics"),
            component: id("reports"),
            descriptor_digest: fabric_component::Digest::try_new(FIRST).unwrap(),
        }
    );
}

#[test]
fn an_unknown_application_or_an_invalid_result_is_refused() {
    let unknown = Catalogue::default()
        .select_component(
            &id("analytics"),
            &id("reports"),
            resolution("1.4.0", FIRST),
            "b",
            1,
        )
        .unwrap_err();
    let mut declared_twice = definition(vec![]);
    declared_twice.fields = resolution("1.4.0", FIRST).descriptor.spec().fields.clone();
    let clash = select(
        &catalogue(declared_twice, vec![]),
        "reports",
        resolution("1.4.0", FIRST),
    )
    .unwrap_err();

    assert_eq!(unknown.to_string(), "catalogue: Application does not exist");
    assert!(clash.to_string().contains("Duplicate field: team"), "{clash}");
}

#[test]
fn apply_refuses_a_selection_it_cannot_resolve() {
    let body = json!({"action":"selectComponentVersion","id":"analytics","component":"reports",
        "repository":"registry.example.com/acme/reports","version":"1.4.0"});
    let command: CatalogueCommand = serde_json::from_value(body.clone()).unwrap();
    let mut with_digest = body;
    with_digest["digest"] = json!(FIRST);

    let error = with(vec![]).apply(command.clone(), "b", 1).unwrap_err();

    assert_eq!(command.operation(), "select_component_version");
    assert!(
        error.to_string().contains("resolved by the server first"),
        "{error}"
    );
    let refused = serde_json::from_value::<CatalogueCommand>(with_digest).unwrap_err();
    assert!(refused.to_string().contains("digest"), "{refused}");
}

#[test]
fn a_release_freezes_the_resolution_by_copy() {
    let selected = select(&with(vec![]), "reports", resolution("1.4.0", FIRST)).unwrap();
    let published = selected
        .apply(
            CatalogueCommand::PublishApplication {
                id: id("analytics"),
                note: "Initial release".into(),
            },
            "b",
            10,
        )
        .unwrap();

    let reselected = select(&published, "reports", resolution("1.5.0", SECOND)).unwrap();

    let application = &reselected.applications[0];
    let released = &application.releases[0].definition.components[0];
    assert_eq!(released.resolution, Some(resolution("1.4.0", FIRST)));
    assert_eq!(application.draft.components[0].version, "1.5.0");
    assert_eq!(
        Catalogue::parse(&reselected.render().unwrap()).unwrap(),
        reselected
    );
}

#[test]
fn a_client_document_holds_its_own_copy_of_the_resolution() {
    let selected = select(&with(vec![]), "reports", resolution("1.4.0", FIRST)).unwrap();
    let published = selected
        .apply(
            CatalogueCommand::PublishApplication {
                id: id("analytics"),
                note: "Initial release".into(),
            },
            "b",
            10,
        )
        .unwrap();
    let request = ClientProductRequest {
        display_name: "Acme".into(),
        hosts: vec![crate::Host::try_new("acme.example.com").unwrap()],
        legal_name: "Acme Ltd".into(),
        region: "NZ".into(),
        timezone: "Pacific/Auckland".into(),
        configuration: ConfigurationValues::default(),
        applications: vec![AssignmentRequest {
            application_id: id("analytics"),
            version: 1,
            plan_id: id("standard"),
            configuration: [("team".to_owned(), "Finance".to_owned())].into(),
        }],
    };
    let product = published.resolve(&request, &[]).unwrap();

    let document = crate::ClientDocument::create(&id("acme"), &request, &product).unwrap();
    let read = crate::ClientDocument::parse(&document.render().unwrap())
        .unwrap()
        .product()
        .unwrap();

    assert_eq!(read, product);
    let copied = &read.applications[0].release.definition.components[0];
    assert_eq!(copied.resolution, Some(resolution("1.4.0", FIRST)));
}
