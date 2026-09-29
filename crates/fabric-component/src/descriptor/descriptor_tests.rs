//! Reading, validating, rendering and writing a component descriptor.
use super::{
    ComponentDescriptor, ComponentSource, Digest, Repository, Role, ARTIFACT_TYPE, MAX_DOCUMENT_BYTES,
    MAX_RESOURCES,
};
use crate::{validate_fields, ConfigurationField, ContractError, FieldKind};
use std::collections::BTreeMap;

const API: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const WEB: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

/// ADR 0026 section 2's example, with real-looking digests substituted.
pub(super) fn example() -> String {
    format!(
        r#"{{
  "apiVersion": "fabric.fieldstate.nz/v1",
  "kind": "Component",
  "spec": {{
    "name": "reports",
    "title": "Reports",
    "description": "Scheduled reporting over a client's own data.",
    "version": "1.4.0",
    "images": {{
      "api": {{ "repository": "registry.example.com/acme/reports",     "digest": "{API}" }},
      "web": {{ "repository": "registry.example.com/acme/reports-web", "digest": "{WEB}" }}
    }},
    "capabilities": ["Identity", "Database"],
    "fields": [ {{ "key": "team", "label": "Team", "kind": "text", "required": true,
                  "default": null, "options": [], "description": "" }} ],
    "resources": [ {{ "name": "reports", "dataSource": "primary", "collection": "reports" }} ]
  }}
}}"#
    )
}

fn read(text: &str) -> Result<ComponentDescriptor, ContractError> {
    ComponentDescriptor::from_json(text.as_bytes())
}

fn refusal(text: &str) -> String {
    read(text).map(|_| ()).unwrap_err().to_string()
}

#[test]
fn the_adrs_example_parses() {
    let descriptor = read(&example()).unwrap();
    let spec = descriptor.spec();

    assert_eq!(spec.name.as_str(), "reports");
    assert_eq!(spec.version.as_str(), "1.4.0");
    assert_eq!(spec.images.len(), 2);
    assert_eq!(spec.resources[0].key_field.as_str(), "id");
    let repository = Repository::try_new("registry.example.com/acme/reports-web").unwrap();
    let role = descriptor.role_of(&repository, &Digest::try_new(WEB).unwrap());
    assert_eq!(role.map(Role::as_str), Some("web"));
    assert_eq!(
        descriptor.role_of(&repository, &Digest::try_new(API).unwrap()),
        None
    );
}

#[test]
fn to_json_is_canonical_and_round_trips() {
    let descriptor = read(&example()).unwrap();

    let bytes = descriptor.to_json();

    assert_eq!(ComponentDescriptor::from_json(&bytes).unwrap(), descriptor);
    assert_eq!(ComponentDescriptor::from_json(&bytes).unwrap().to_json(), bytes);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with(
        r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{"name":"reports","#
    ));
    assert!(
        text.contains(r#""keyField":"id","operations":["read","list"],"queryableFields":[]"#),
        "{text}"
    );
    assert!(!text.contains('\n'), "{text}");
}

#[test]
fn a_duplicate_key_is_refused_at_every_depth() {
    let example = example();
    let cases = [
        example.replacen(
            r#""kind": "Component","#,
            r#""kind": "Component", "kind": "Component","#,
            1,
        ),
        example.replacen(
            r#""title": "Reports","#,
            r#""title": "Reports", "title": "Other","#,
            1,
        ),
        example.replacen(r#""web": {"#, r#""api": {"#, 1),
        example.replacen(r#""label": "Team","#, r#""label": "Team", "label": "Team","#, 1),
    ];
    for case in cases {
        assert_ne!(case, example);
        let error = refusal(&case);
        assert!(error.contains("appears twice"), "{error}");
    }
}

#[test]
fn an_oversized_document_is_refused_before_it_is_parsed() {
    let oversized = vec![b'{'; MAX_DOCUMENT_BYTES + 1];

    let error = ComponentDescriptor::from_json(&oversized)
        .unwrap_err()
        .to_string();

    assert!(error.contains("at most 16384 bytes"), "{error}");
}

#[test]
fn text_that_is_not_utf8_or_not_one_value_is_refused() {
    assert!(ComponentDescriptor::from_json(b"{\"a\": \"\xff\"}").is_err());
    assert!(refusal(&format!("{} {{}}", example())).contains("not valid JSON"));
}

#[test]
fn the_wrong_document_is_refused_as_the_wrong_document() {
    let catalogue = example().replacen(r#""kind": "Component""#, r#""kind": "Catalogue""#, 1);
    let error = refusal(&catalogue);
    assert_eq!(
        error,
        "expected fabric.fieldstate.nz/v1/Component, found fabric.fieldstate.nz/v1/Catalogue"
    );

    let error = refusal(r#"{"spec": {}}"#);
    assert!(error.contains("no apiVersion or kind at all"), "{error}");
    assert!(refusal("[]").contains("expected fabric.fieldstate.nz/v1/Component"));
}

#[test]
fn a_later_api_version_is_unsupported_and_named() {
    let later = example().replacen("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v2", 1);

    let error = read(&later).unwrap_err();

    assert_eq!(error, ContractError::UnsupportedVersion { found: "v2".into() });
    assert!(error.to_string().contains("version v2"), "{error}");
    for not_a_version in [
        "fabric.fieldstate.nz/",
        "fabric.fieldstate.nz/beta",
        "fabric.fieldstate.nz/v02",
    ] {
        let other = example().replacen("fabric.fieldstate.nz/v1", not_a_version, 1);
        assert!(
            matches!(read(&other), Err(ContractError::Invalid { .. })),
            "{not_a_version}"
        );
    }
    let other_group = example().replacen("fabric.fieldstate.nz/v1", "example.com/v1", 1);
    assert!(matches!(read(&other_group), Err(ContractError::Invalid { .. })));
}

#[test]
fn an_artifact_of_a_later_version_is_unsupported() {
    let bytes = example().into_bytes();

    let error =
        ComponentDescriptor::from_artifact("application/vnd.saas-fabric.component.v2", &bytes).unwrap_err();

    assert_eq!(error, ContractError::UnsupportedVersion { found: "v2".into() });
    assert!(ComponentDescriptor::from_artifact(ARTIFACT_TYPE, &bytes).is_ok());
    assert!(matches!(
        ComponentDescriptor::from_artifact("application/vnd.oci.image.manifest.v1+json", &bytes),
        Err(ContractError::Invalid { .. })
    ));
    let later = example().replacen("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v2", 1);
    let error = ComponentDescriptor::from_artifact(ARTIFACT_TYPE, later.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("disagrees"), "{error}");
}

#[test]
fn an_unknown_field_is_refused() {
    let error = refusal(&example().replacen(
        r#""title": "Reports","#,
        r#""title": "Reports", "modules": [],"#,
        1,
    ));

    assert!(error.contains("modules"), "{error}");
}

#[test]
fn format_characters_are_refused_in_declared_text() {
    let title = refusal(&example().replacen(r#""title": "Reports""#, "\"title\": \"Rep\u{202E}orts\"", 1));
    let label = refusal(&example().replacen(r#""label": "Team""#, "\"label\": \"Te\u{200B}am\"", 1));

    assert!(
        title.contains("Component title must not contain Unicode format characters"),
        "{title}"
    );
    assert!(
        label.contains("Field label must not contain Unicode format characters"),
        "{label}"
    );
}

#[test]
fn the_shared_field_validator_does_not_refuse_format_characters() {
    let field = ConfigurationField {
        key: "team".into(),
        label: "Te\u{200B}am".into(),
        kind: FieldKind::Text,
        required: false,
        default: Some("\u{202E}x".into()),
        options: vec![],
        description: "\u{FEFF}".into(),
    };

    validate_fields(&[field]).unwrap();
}

#[test]
fn images_on_two_registries_are_refused() {
    let error = refusal(&example().replacen(
        "registry.example.com/acme/reports-web",
        "ghcr.io/acme/reports-web",
        1,
    ));

    assert!(error.contains("one registry"), "{error}");
    let port = refusal(&example().replacen(
        "registry.example.com/acme/reports-web",
        "registry.example.com:5000/acme/reports-web",
        1,
    ));
    assert!(port.contains("one registry"), "{port}");
}

#[test]
fn counts_and_repeats_are_bounded() {
    let empty = refusal(&with_images(&example(), "{}"));
    assert!(empty.contains("between 1 and 8 images"), "{empty}");
    let repeated =
        refusal(&example().replacen(r#"["Identity", "Database"]"#, r#"["Identity", "Identity"]"#, 1));
    assert!(repeated.contains("Duplicate capability: Identity"), "{repeated}");
    let unknown = refusal(&example().replacen(r#""Database""#, r#""Queues""#, 1));
    assert!(unknown.contains("Queues"), "{unknown}");
}

/// Replaces the `images` object with `images`.
fn with_images(text: &str, images: &str) -> String {
    let start = text.find(r#""images": {"#).unwrap() + r#""images": "#.len();
    let end = text.find(r#""capabilities""#).unwrap();
    format!("{}{images},\n    {}", &text[..start], &text[end..])
}

const SOURCE: &str = "apiVersion: fabric.fieldstate.nz/v1
kind: Component
spec:
  name: reports
  title: Reports
  images:
    api:
      repository: registry.example.com/acme/reports
    web:
      repository: registry.example.com/acme/reports-web
";

fn digests(roles: &[(&str, &str)]) -> BTreeMap<Role, Digest> {
    roles
        .iter()
        .map(|(role, digest)| (Role::try_new(role).unwrap(), Digest::try_new(digest).unwrap()))
        .collect()
}

#[test]
fn a_source_renders_a_descriptor() {
    let source = ComponentSource::from_yaml(SOURCE).unwrap();

    let descriptor = source
        .render("1.4.0", &digests(&[("api", API), ("web", WEB)]))
        .unwrap();

    assert_eq!(descriptor.spec().images.len(), 2);
    assert!(descriptor.spec().capabilities.is_empty());
    assert_eq!(
        ComponentDescriptor::from_json(&descriptor.to_json()).unwrap(),
        descriptor
    );
    let images: Vec<(&str, &str)> = source
        .images()
        .map(|(role, repository)| (role.as_str(), repository.as_str()))
        .collect();
    assert_eq!(images[1], ("web", "registry.example.com/acme/reports-web"));
}

#[test]
fn rendering_needs_exactly_the_sources_roles() {
    let source = ComponentSource::from_yaml(SOURCE).unwrap();

    let missing = source
        .render("1.4.0", &digests(&[("api", API)]))
        .unwrap_err()
        .to_string();
    let extra = source
        .render("1.4.0", &digests(&[("api", API), ("web", WEB), ("worker", WEB)]))
        .unwrap_err()
        .to_string();

    assert!(missing.contains("missing: [web]; extra: []"), "{missing}");
    assert!(extra.contains("missing: []; extra: [worker]"), "{extra}");
    assert!(source
        .render("v1.4.0", &digests(&[("api", API), ("web", WEB)]))
        .is_err());
}

#[test]
fn a_source_checks_its_envelope_and_fields() {
    let wrong = SOURCE.replacen("kind: Component", "kind: Catalogue", 1);
    let versioned = SOURCE.replacen("  title: Reports\n", "  title: Reports\n  version: 1.0.0\n", 1);

    assert!(ComponentSource::from_yaml(&wrong).is_err());
    assert!(ComponentSource::from_yaml(&versioned)
        .unwrap_err()
        .to_string()
        .contains("version"));
}

/// A document within the bound as read whose canonical form, which writes
/// every resource's defaults, is over it.
fn within_the_bound_until_its_defaults_are_written() -> String {
    let resources: Vec<String> = (0..MAX_RESOURCES)
        .map(|n| format!(r#"{{"name":"r{n}","dataSource":"primary","collection":"c{n}"}}"#))
        .collect();
    let fields: Vec<String> = (0..11)
        .map(|n| {
            format!(
                r#"{{"key":"f{n}","label":"F","kind":"text","required":false,"default":null,"options":[],"description":"{}"}}"#,
                "d".repeat(1000)
            )
        })
        .collect();
    format!(
        r#"{{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{{"name":"reports","title":"Reports","version":"1.4.0","images":{{"api":{{"repository":"registry.example.com/acme/reports","digest":"{API}"}}}},"fields":[{}],"resources":[{}]}}}}"#,
        fields.join(","),
        resources.join(",")
    )
}

#[test]
fn a_descriptor_whose_canonical_form_is_over_the_bound_is_refused_however_it_is_made() {
    let text = within_the_bound_until_its_defaults_are_written();
    assert!(text.len() <= MAX_DOCUMENT_BYTES, "{}", text.len());

    let error = refusal(&text);

    assert!(error.contains("canonical form is"), "{error}");
    assert!(error.contains("at most 16384 are read"), "{error}");
    let mut spec = read(&example()).unwrap().spec().clone();
    spec.fields[0].kind = FieldKind::Choice;
    spec.fields[0].options = (0..2000).map(|n| format!("option-{n}")).collect();
    let error = ComponentDescriptor::new(spec).unwrap_err().to_string();
    assert!(error.contains("canonical form is"), "{error}");
}

#[test]
fn every_descriptor_that_exists_reads_back_from_its_own_bytes() {
    let fits = within_the_bound_until_its_defaults_are_written().replacen(
        &format!(r#""description":"{}""#, "d".repeat(1000)),
        r#""description":"""#,
        11,
    );

    let descriptor = read(&fits).unwrap();

    assert_eq!(
        ComponentDescriptor::from_json(&descriptor.to_json()).unwrap(),
        descriptor
    );
}

#[test]
fn one_repository_is_named_by_one_role() {
    let error = refusal(&example().replacen(
        "registry.example.com/acme/reports-web",
        "registry.example.com/acme/reports",
        1,
    ));

    assert_eq!(
        error,
        "Duplicate image repository: registry.example.com/acme/reports"
    );
}
