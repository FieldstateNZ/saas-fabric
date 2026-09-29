//! Which manifests are held to be a component descriptor's.

use fabric_platform_management::Unusable;
use serde_json::{json, Value};

use super::candidate::Candidate;
use super::shape::check;
use crate::client::digest::Content;

const SUBJECT: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const LAYER: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn manifest() -> Value {
    json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "artifactType": "application/vnd.saas-fabric.component.v1",
        "config": {
            "mediaType": "application/vnd.oci.empty.v1+json",
            "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
            "size": 2
        },
        "layers": [{
            "mediaType": "application/vnd.saas-fabric.component.v1+json",
            "digest": LAYER,
            "size": 100
        }],
        "subject": { "mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": SUBJECT, "size": 1 },
        "annotations": {
            "org.opencontainers.image.revision": "5320432",
            "org.opencontainers.image.version": "1.4.0"
        }
    })
}

fn checked(value: &Value) -> Candidate {
    check(SUBJECT, &Content::hashed(value.to_string().into_bytes()))
}

fn malformed_when(change: impl FnOnce(&mut Value)) {
    let mut value = manifest();
    change(&mut value);
    assert_eq!(
        checked(&value),
        Candidate::Unusable(Unusable::Malformed),
        "{value}"
    );
}

#[test]
fn a_component_descriptor_manifest_is_usable_and_says_what_it_is() {
    let Candidate::Usable(found) = checked(&manifest()) else {
        panic!("usable");
    };

    assert_eq!(found.artifact_type, "application/vnd.saas-fabric.component.v1");
    assert_eq!(found.layer_digest, LAYER);
    assert_eq!(found.layer_size, 100);
    assert_eq!(found.revision.as_deref(), Some("5320432"));
    assert_eq!(found.version.as_deref(), Some("1.4.0"));
}

#[test]
fn a_version_this_build_does_not_read_is_still_one_of_the_family() {
    let mut value = manifest();
    value["artifactType"] = json!("application/vnd.saas-fabric.component.v2");
    value["layers"][0]["mediaType"] = json!("application/vnd.saas-fabric.component.v2+json");

    assert!(matches!(checked(&value), Candidate::Usable(_)));
}

#[test]
fn another_subject_is_named_as_such() {
    let mut value = manifest();
    value["subject"]["digest"] = json!(LAYER);

    assert_eq!(checked(&value), Candidate::Unusable(Unusable::OtherSubject));
}

#[test]
fn every_other_shape_failure_is_malformed() {
    malformed_when(|value| value["mediaType"] = Value::Null);
    malformed_when(|value| value["mediaType"] = json!("application/vnd.oci.image.index.v1+json"));
    malformed_when(|value| value["artifactType"] = json!("application/vnd.cncf.notary.signature"));
    malformed_when(|value| value["config"]["mediaType"] = json!("application/vnd.oci.image.config.v1+json"));
    malformed_when(|value| {
        let layer = value["layers"][0].clone();
        value["layers"] = json!([layer.clone(), layer]);
    });
    malformed_when(|value| value["layers"] = json!([]));
    malformed_when(|value| value["layers"][0]["mediaType"] = json!("application/json"));
    malformed_when(|value| value["layers"][0]["size"] = json!(16 * 1024 + 1));
    malformed_when(|value| value["layers"][0]["size"] = Value::Null);
    malformed_when(|value| value["layers"][0]["digest"] = json!("sha512:abcd"));
    malformed_when(|value| value["subject"] = Value::Null);
    malformed_when(|value| *value = json!("not a manifest"));
}

#[test]
fn its_revision_is_read_by_the_rule_every_image_revision_is_read_by() {
    let revision_of = |stamped: &str| {
        let mut value = manifest();
        value["annotations"]["org.opencontainers.image.revision"] = json!(stamped);
        let Candidate::Usable(found) = checked(&value) else {
            panic!("usable");
        };
        found.revision
    };

    assert_eq!(revision_of("5320432\n").as_deref(), Some("5320432"));
    assert_eq!(revision_of("   "), None, "blank is no revision");
    assert_eq!(revision_of(""), None);
}
