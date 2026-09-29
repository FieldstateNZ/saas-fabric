//! Pins component descriptor v1's shape, so a change to it fails here
//! rather than shipping inside v1.
//!
//! # Why these tests exist
//!
//! v1's field set and enumerations are fixed (ADR 0026 section 2): a new
//! field, even an optional one, or a new enumeration value is a new version.
//! Nothing else would notice one. A `#[serde(default, skip_serializing_if)]`
//! field added to a shared shape changes no existing document's bytes, and a
//! new `FieldKind` is satisfied by one more arm where it is checked. So the
//! canonical bytes of the ADR's example are pinned whole, every struct a
//! descriptor carries is destructured without `..`, and every enumeration is
//! matched without a wildcard. A compile error or a failure here means: this
//! is a new component descriptor version, not an edit to v1.
use super::descriptor_tests::example;
use super::{ComponentDescriptor, ComponentSpec, ImageReference};
use crate::{ApplicationResource, ConfigurationField, FieldKind, PlatformCapability};

/// The canonical bytes of ADR 0026 section 2's example, digests substituted.
const CANONICAL: &str = concat!(
    r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{"#,
    r#""name":"reports","title":"Reports","#,
    r#""description":"Scheduled reporting over a client's own data.","version":"1.4.0","#,
    r#""images":{"#,
    r#""api":{"repository":"registry.example.com/acme/reports","#,
    r#""digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"},"#,
    r#""web":{"repository":"registry.example.com/acme/reports-web","#,
    r#""digest":"sha256:2222222222222222222222222222222222222222222222222222222222222222"}},"#,
    r#""capabilities":["Identity","Database"],"#,
    r#""fields":[{"key":"team","label":"Team","kind":"text","required":true,"#,
    r#""default":null,"options":[],"description":""}],"#,
    r#""resources":[{"name":"reports","dataSource":"primary","collection":"reports","#,
    r#""keyField":"id","operations":["read","list"],"queryableFields":[]}]}}"#,
);

#[test]
fn the_adrs_example_has_exactly_these_canonical_bytes() {
    let descriptor = ComponentDescriptor::from_json(example().as_bytes()).unwrap();

    assert_eq!(String::from_utf8(descriptor.to_json()).unwrap(), CANONICAL);
}

#[test]
fn every_struct_a_descriptor_carries_has_exactly_v1s_fields() {
    let descriptor = ComponentDescriptor::from_json(CANONICAL.as_bytes()).unwrap();
    // No `..` below: a field added to any of these fails to compile here.
    let ComponentSpec {
        name,
        title,
        description,
        version,
        images,
        capabilities,
        fields,
        resources,
    } = descriptor.spec().clone();
    let ConfigurationField {
        key,
        label,
        kind,
        required,
        default,
        options,
        description: help,
    } = fields[0].clone();
    let ApplicationResource {
        name: resource,
        data_source,
        collection,
        key_field,
        operations,
        queryable_fields,
    } = resources[0].clone();
    let ImageReference { repository, digest } = images.values().next().unwrap().clone();

    assert_eq!(
        (name.as_str(), title.as_str(), version.as_str()),
        ("reports", "Reports", "1.4.0")
    );
    assert!(!description.is_empty());
    assert_eq!(
        capabilities,
        [PlatformCapability::Identity, PlatformCapability::Database]
    );
    assert_eq!(
        (key.as_str(), label.as_str(), kind, required),
        ("team", "Team", FieldKind::Text, true)
    );
    assert_eq!((default, options.len(), help.as_str()), (None, 0, ""));
    assert_eq!(
        (
            resource.as_str(),
            data_source.as_str(),
            collection.as_str(),
            key_field.as_str()
        ),
        ("reports", "primary", "reports", "id")
    );
    assert_eq!((operations.len(), queryable_fields.len()), (2, 0));
    assert_eq!(repository.as_str(), "registry.example.com/acme/reports");
    assert!(digest.as_str().starts_with("sha256:1111"));
}

/// Every `FieldKind`, by its serialized name. No wildcard: a new kind fails
/// to compile here, because it is a new component descriptor version.
fn field_kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text => "text",
        FieldKind::Number => "number",
        FieldKind::Boolean => "boolean",
        FieldKind::Choice => "choice",
        FieldKind::Hostname => "hostname",
        FieldKind::Identifier => "identifier",
        FieldKind::Timezone => "timezone",
    }
}

#[test]
fn field_kinds_are_exactly_v1s_seven() {
    let kinds = [
        FieldKind::Text,
        FieldKind::Number,
        FieldKind::Boolean,
        FieldKind::Choice,
        FieldKind::Hostname,
        FieldKind::Identifier,
        FieldKind::Timezone,
    ];

    for kind in kinds {
        let written = serde_json::to_string(&kind).unwrap();
        assert_eq!(written, format!("\"{}\"", field_kind_name(kind)));
    }
}

/// Every `PlatformCapability`, by its serialized name. No wildcard, for the
/// same reason as [`field_kind_name`].
fn capability_name(capability: PlatformCapability) -> &'static str {
    match capability {
        PlatformCapability::Identity => "Identity",
        PlatformCapability::Database => "Database",
        PlatformCapability::Secrets => "Secrets",
        PlatformCapability::Authorization => "Authorization",
        PlatformCapability::Routing => "Routing",
        PlatformCapability::ObjectStorage => "Object storage",
        PlatformCapability::Messaging => "Messaging",
    }
}

#[test]
fn capabilities_are_exactly_v1s_seven() {
    assert_eq!(PlatformCapability::ALL.len(), 7);
    for capability in PlatformCapability::ALL {
        let written = serde_json::to_string(&capability).unwrap();
        assert_eq!(written, format!("\"{}\"", capability_name(capability)));
    }
}
