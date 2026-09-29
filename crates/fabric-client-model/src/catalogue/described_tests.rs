//! A described component in the catalogue: its resolution's rules, and the
//! `apiVersion` a catalogue holding one is written at. The builders here are
//! shared by the other ADR 0026 test files beside it.
use super::*;
use crate::{ClientId, DesiredStateError};
use fabric_component::{ComponentDescriptor, ComponentVersion, Digest, Repository};

pub(super) const REPOSITORY: &str = "registry.example.com/acme/reports";
pub(super) const PRIMARY: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
pub(super) const FIRST: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
pub(super) const SECOND: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";
/// A required text field, `team`.
pub(super) const TEAM: &str = r#"[{"key":"team","label":"Team","kind":"text","required":true,"default":null,"options":[],"description":""}]"#;
/// A resource, `reports`.
pub(super) const REPORTS: &str = r#"[{"name":"reports","dataSource":"primary","collection":"reports"}]"#;

pub(super) fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

/// A component descriptor at `version` naming [`REPOSITORY`] at [`PRIMARY`].
pub(super) fn descriptor(version: &str, fields: &str, resources: &str) -> ComponentDescriptor {
    let json = format!(
        r#"{{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{{"name":"reports","title":"Reports","version":"{version}","images":{{"api":{{"repository":"{REPOSITORY}","digest":"{PRIMARY}"}}}},"fields":{fields},"resources":{resources}}}}}"#
    );
    ComponentDescriptor::from_json(json.as_bytes()).unwrap()
}

/// A resolution at `version`, its descriptor declaring [`TEAM`] and
/// [`REPORTS`].
pub(super) fn resolution(version: &str, descriptor_digest: &str) -> ComponentResolution {
    resolution_declaring(version, descriptor_digest, TEAM, REPORTS)
}

pub(super) fn resolution_declaring(
    version: &str,
    descriptor_digest: &str,
    fields: &str,
    resources: &str,
) -> ComponentResolution {
    ComponentResolution {
        repository: Repository::try_new(REPOSITORY).unwrap(),
        version: ComponentVersion::try_new(version).unwrap(),
        primary_digest: Digest::try_new(PRIMARY).unwrap(),
        descriptor_digest: Digest::try_new(descriptor_digest).unwrap(),
        revision: "c".repeat(40),
        resolved_at: 1_700_000_000,
        descriptor: descriptor(version, fields, resources),
    }
}

pub(super) fn described(component: &str, resolution: ComponentResolution) -> ApplicationComponent {
    ApplicationComponent {
        id: id(component),
        name: "Reports".into(),
        kind: ComponentKind::Described,
        reference: resolution.repository.to_string(),
        version: resolution.version.to_string(),
        required: false,
        policy: UpdatePolicy::Manual,
        resolution: Some(resolution),
    }
}

pub(super) fn authored(component: &str, kind: ComponentKind) -> ApplicationComponent {
    ApplicationComponent {
        id: id(component),
        name: "Authored".into(),
        kind,
        reference: if kind == ComponentKind::Capability {
            "Identity"
        } else {
            "registry.example.com/web"
        }
        .into(),
        version: if kind == ComponentKind::Capability {
            ""
        } else {
            "1.0.0"
        }
        .into(),
        required: true,
        policy: UpdatePolicy::Automatic,
        resolution: None,
    }
}

/// A publishable definition with one plan and `components`.
pub(super) fn definition(components: Vec<ApplicationComponent>) -> ApplicationDefinition {
    ApplicationDefinition {
        name: "Analytics".into(),
        components,
        plans: vec![ApplicationPlan {
            id: id("standard"),
            name: "Standard".into(),
            description: String::new(),
            features: vec![],
            configuration: ConfigurationValues::default(),
        }],
        ..ApplicationDefinition::default()
    }
}

/// A catalogue of one application, `analytics`, with `draft` and `releases`.
pub(super) fn catalogue(draft: ApplicationDefinition, releases: Vec<ApplicationDefinition>) -> Catalogue {
    let releases = releases
        .into_iter()
        .zip(1..)
        .map(|(definition, version)| ApplicationRelease {
            version,
            note: String::new(),
            published_at: 0,
            definition,
        })
        .collect();
    Catalogue {
        applications: vec![Application {
            id: id("analytics"),
            draft,
            releases,
        }],
        ..Catalogue::default()
    }
}

fn refusal(definition: &ApplicationDefinition) -> String {
    definition.validate(false).unwrap_err().to_string()
}

#[test]
fn a_described_component_validates_with_its_resolution() {
    definition(vec![described("reports", resolution("1.4.0", FIRST))])
        .validate(true)
        .unwrap();
}

#[test]
fn a_described_component_has_a_resolution_and_no_other_kind_has_one() {
    let mut bare = described("reports", resolution("1.4.0", FIRST));
    bare.resolution = None;
    let mut carrying = authored("web", ComponentKind::Container);
    carrying.resolution = Some(resolution("1.4.0", FIRST));

    assert!(refusal(&definition(vec![bare])).contains("has no resolution"));
    assert!(refusal(&definition(vec![carrying])).contains("cannot carry a resolution"));
}

#[test]
fn the_component_agrees_with_its_resolution() {
    let mut elsewhere = described("reports", resolution("1.4.0", FIRST));
    elsewhere.reference = "registry.example.com/acme/other".into();
    let mut later = described("reports", resolution("1.4.0", FIRST));
    later.version = "1.5.0".into();

    for component in [elsewhere, later] {
        let error = refusal(&definition(vec![component]));
        assert!(
            error.contains("was resolved from registry.example.com/acme/reports at 1.4.0"),
            "{error}"
        );
    }
}

#[test]
fn the_frozen_descriptor_agrees_with_the_resolution() {
    let mut other_version = resolution("1.4.0", FIRST);
    other_version.descriptor = descriptor("1.5.0", "[]", "[]");
    let mut other_digest = resolution("1.4.0", FIRST);
    other_digest.primary_digest = Digest::try_new(SECOND).unwrap();
    let mut no_revision = resolution("1.4.0", FIRST);
    no_revision.revision = String::new();

    let describes = refusal(&definition(vec![described("reports", other_version)]));
    let names = refusal(&definition(vec![described("reports", other_digest)]));
    let revision = refusal(&definition(vec![described("reports", no_revision)]));

    assert!(
        describes.contains("its component descriptor describes 1.5.0"),
        "{describes}"
    );
    assert!(
        names.contains(&format!("does not name {REPOSITORY} at {SECOND}")),
        "{names}"
    );
    assert!(revision.contains("Component revision"), "{revision}");
}

#[test]
fn a_name_in_effect_twice_is_refused_naming_both_sources() {
    let mut both = definition(vec![described("reports", resolution("1.4.0", FIRST))]);
    both.fields = descriptor("1.4.0", TEAM, "[]").spec().fields.clone();
    let two = definition(vec![
        described("reports", resolution("1.4.0", FIRST)),
        described("more", resolution_declaring("1.4.0", SECOND, "[]", REPORTS)),
    ]);

    assert_eq!(
        refusal(&both),
        "catalogue: Duplicate field: team is authored on the application and declared by component 'reports'"
    );
    assert_eq!(
        refusal(&two),
        "catalogue: Duplicate resource: reports is declared by component 'reports' and declared by component 'more'"
    );
}

#[test]
fn a_catalogue_holding_a_described_component_is_written_at_v2_and_reads_back() {
    let catalogue = catalogue(
        definition(vec![described("reports", resolution("1.4.0", FIRST))]),
        vec![],
    );

    let text = catalogue.render().unwrap();

    assert!(
        text.starts_with("apiVersion: fabric.fieldstate.nz/v2\nkind: Catalogue\n"),
        "{text}"
    );
    assert!(
        text.contains("          descriptor:\n            apiVersion: fabric.fieldstate.nz/v1\n"),
        "{text}"
    );
    assert_eq!(Catalogue::parse(&text).unwrap(), catalogue);
}

#[test]
fn a_described_component_only_in_a_release_still_needs_v2() {
    let released = definition(vec![described("reports", resolution("1.4.0", FIRST))]);
    let catalogue = catalogue(definition(vec![]), vec![released]);

    assert!(catalogue
        .render()
        .unwrap()
        .starts_with("apiVersion: fabric.fieldstate.nz/v2\n"));
}

#[test]
fn a_v2_catalogue_without_one_re_renders_as_v1() {
    let v1 = catalogue(
        definition(vec![authored("web", ComponentKind::Container)]),
        vec![],
    )
    .render()
    .unwrap();
    let v2 = v1.replacen("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v2", 1);

    assert_eq!(Catalogue::parse(&v2).unwrap().render().unwrap(), v1);
}

#[test]
fn a_described_component_under_v1_is_refused_naming_the_version() {
    let v2 = catalogue(
        definition(vec![described("reports", resolution("1.4.0", FIRST))]),
        vec![],
    )
    .render()
    .unwrap();
    let v1 = v2.replacen("fabric.fieldstate.nz/v2", "fabric.fieldstate.nz/v1", 1);

    let error = Catalogue::parse(&v1).unwrap_err();

    assert!(
        matches!(error, DesiredStateError::CatalogueMalformed { .. }),
        "{error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("component 'reports' of application 'analytics'"),
        "{message}"
    );
    assert!(
        message.contains("fabric.fieldstate.nz/v1 cannot express"),
        "{message}"
    );
    assert!(message.contains("fabric.fieldstate.nz/v2"), "{message}");
}

#[test]
fn a_hand_edit_that_breaks_a_resolution_makes_the_catalogue_unreadable() {
    let text = catalogue(
        definition(vec![described("reports", resolution("1.4.0", FIRST))]),
        vec![],
    )
    .render()
    .unwrap();
    let frozen = "            apiVersion: fabric.fieldstate.nz/v1\n";
    let title = "              title: Reports\n";
    let edits = [
        (
            text.replacen(
                &format!("reference: {REPOSITORY}\n"),
                "reference: registry.example.com/acme/other\n",
                1,
            ),
            "was resolved from",
        ),
        (
            text.replacen("          version: 1.4.0\n", "          version: 1.5.0\n", 1),
            "was resolved from",
        ),
        (
            text.replacen(
                &format!("primaryDigest: {PRIMARY}"),
                &format!("primaryDigest: {SECOND}"),
                1,
            ),
            "does not name",
        ),
        (
            text.replacen(title, "              title: \"Rep\\u202Eorts\"\n", 1),
            "Unicode format characters",
        ),
        (
            text.replacen(title, &format!("{title}              modules: []\n"), 1),
            "modules",
        ),
        (
            text.replacen(title, &format!("{title}              title: Other\n"), 1),
            "duplicate",
        ),
        (
            text.replacen(frozen, "            apiVersion: fabric.fieldstate.nz/v2\n", 1),
            "version v2",
        ),
    ];
    for (edit, expected) in edits {
        assert_ne!(edit, text);
        let error = Catalogue::parse(&edit).unwrap_err().to_string();
        assert!(error.contains(expected), "{expected}: {error}");
    }
}
