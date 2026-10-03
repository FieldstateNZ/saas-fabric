//! Saving a draft keeps, drops and refuses as ADR 0026 section 7 says, and
//! its body cannot carry what the server resolves.
use super::described_tests::{authored, catalogue, definition, described, resolution, FIRST};
use super::*;
use serde_json::{json, Value};

fn container() -> Value {
    json!({"id":"web","name":"Web","kind":"container","reference":"registry.example.com/web","version":"1.0.0","required":true,"policy":"automatic"})
}

fn described_body() -> Value {
    json!({"id":"reports","name":"Renamed","kind":"described","required":true,"policy":"automatic"})
}

fn body(components: &[Value]) -> Value {
    json!({"name":"Analytics","description":"","domain":"","components":components,
        "features":[],"plans":[{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields":[],"navigation":[]})
}

fn draft(components: &[Value]) -> ApplicationDraft {
    serde_json::from_value(body(components)).unwrap()
}

fn refused(components: &[Value]) -> String {
    serde_json::from_value::<ApplicationDraft>(body(components))
        .unwrap_err()
        .to_string()
}

fn stored() -> ApplicationDefinition {
    definition(vec![
        described("reports", resolution("1.4.0", FIRST)),
        authored("web", ComponentKind::Container),
    ])
}

#[test]
fn an_authored_component_is_todays_shape_exactly() {
    let saved = draft(&[container()]).into_definition(&stored()).unwrap();

    let component = &saved.components[0];
    assert_eq!(component.kind, ComponentKind::Container);
    assert_eq!(component.reference, "registry.example.com/web");
    assert_eq!(component.resolution, None);
    let mut with_resolution = container();
    with_resolution["resolution"] = json!(null);
    assert!(refused(&[with_resolution]).contains("resolution"));
}

#[test]
fn a_described_body_carrying_what_the_server_resolves_is_refused() {
    for field in [
        "reference",
        "version",
        "resolution",
        "primaryDigest",
        "descriptorDigest",
        "fields",
    ] {
        let mut component = described_body();
        component[field] = json!("anything");

        let error = refused(&[component]);

        assert!(error.contains(field), "{field}: {error}");
    }
}

#[test]
fn a_described_component_the_save_names_keeps_its_stored_resolution() {
    let saved = draft(&[described_body()]).into_definition(&stored()).unwrap();

    let component = &saved.components[0];
    assert_eq!(component.name, "Renamed");
    assert!(component.required);
    assert_eq!(component.policy, UpdatePolicy::Automatic);
    assert_eq!(component.resolution, stored().components[0].resolution);
    assert_eq!(component.reference, stored().components[0].reference);
    assert_eq!(component.version, "1.4.0");
    saved.validate(true).unwrap();
}

#[test]
fn a_component_the_save_omits_is_dropped() {
    let saved = draft(&[container()]).into_definition(&stored()).unwrap();

    assert_eq!(saved.components.len(), 1);
    assert_eq!(saved.components[0].id.as_str(), "web");
}

#[test]
fn a_described_component_with_no_stored_resolution_is_refused() {
    let mut new = described_body();
    new["id"] = json!("fresh");

    let error = draft(&[new]).into_definition(&stored()).unwrap_err().to_string();

    assert!(
        error.contains("'fresh' is described and has no stored resolution"),
        "{error}"
    );
}

#[test]
fn a_save_cannot_change_a_components_kind_in_either_direction() {
    let mut to_container = container();
    to_container["id"] = json!("reports");
    let mut to_described = described_body();
    to_described["id"] = json!("web");
    let mut to_helm = container();
    to_helm["kind"] = json!("helm");

    let cases = [
        (
            vec![to_container],
            "'reports' is described and a save cannot make it container",
        ),
        (
            vec![to_described],
            "'web' is container and a save cannot make it described",
        ),
        (vec![to_helm], "'web' is container and a save cannot make it helm"),
    ];
    for (components, expected) in cases {
        let error = draft(&components)
            .into_definition(&stored())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn saving_through_apply_uses_the_same_rules() {
    let catalogue = catalogue(stored(), vec![]);
    let save = |components: &[Value]| CatalogueCommand::SaveApplication {
        id: super::described_tests::id("analytics"),
        definition: draft(components),
    };

    let saved = catalogue
        .apply(save(&[described_body()]), "brett@example.com", 5)
        .unwrap();
    let mut new = described_body();
    new["id"] = json!("fresh");
    let refused = catalogue.apply(save(&[new]), "brett@example.com", 5);

    assert_eq!(
        saved.applications[0].draft.components[0].resolution,
        stored().components[0].resolution
    );
    assert_eq!(saved.activity.last().unwrap().action, "Application draft saved");
    assert!(refused.is_err());
}

#[test]
fn the_save_command_body_is_what_it_was_for_authored_components() {
    let command: CatalogueCommand = serde_json::from_value(json!({
        "action": "saveApplication", "id": "analytics", "definition": body(&[container()])
    }))
    .unwrap();

    assert_eq!(command.operation(), "save_application");
}
