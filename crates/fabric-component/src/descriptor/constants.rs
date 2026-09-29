//! The names and bounds of component descriptor v1 (ADR 0026 sections 1 and
//! 2). Every one is part of the published contract: changing one is a new
//! version, except a bound that only grows.

/// The `artifactType` of a v1 component descriptor's OCI manifest.
pub const ARTIFACT_TYPE: &str = "application/vnd.saas-fabric.component.v1";

/// The media type of the one layer that holds the document.
pub const DOCUMENT_MEDIA_TYPE: &str = "application/vnd.saas-fabric.component.v1+json";

/// What every version's `artifactType` starts with, followed by its number.
/// A reader counts attached component descriptors across the whole family,
/// so a version it does not read is still seen, and named.
pub const ARTIFACT_TYPE_FAMILY_PREFIX: &str = "application/vnd.saas-fabric.component.v";

/// The document's `apiVersion`, which must agree with its `artifactType`.
pub const API_VERSION: &str = "fabric.fieldstate.nz/v1";

/// The document's `kind`.
pub const KIND: &str = "Component";

/// The fixed file name the rendered document is written under.
pub const DOCUMENT_FILE_NAME: &str = "component.json";

/// The largest document, in bytes, that is read at all.
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024;

/// The most images one component may name.
pub const MAX_IMAGES: usize = 8;

/// The most configuration fields one component may declare.
pub const MAX_FIELDS: usize = 64;

/// The most Data API resources one component may declare.
pub const MAX_RESOURCES: usize = 64;

/// The version number an `artifactType` of the component descriptor family
/// names -- `"1"` for [`ARTIFACT_TYPE`], `"2"` for the next -- or `None` for
/// an artifact type outside the family.
///
/// A member of the family is the prefix followed by one or more ASCII
/// digits and nothing else, so `…component.v1+json`, a media type rather
/// than an artifact type, is not one.
#[must_use]
pub fn family_version(artifact_type: &str) -> Option<&str> {
    artifact_type
        .strip_prefix(ARTIFACT_TYPE_FAMILY_PREFIX)
        .filter(|version| !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_names_its_version() {
        assert_eq!(family_version(ARTIFACT_TYPE), Some("1"));
        assert_eq!(
            family_version("application/vnd.saas-fabric.component.v2"),
            Some("2")
        );
        assert_eq!(
            family_version("application/vnd.saas-fabric.component.v12"),
            Some("12")
        );
    }

    #[test]
    fn other_types_are_outside_the_family() {
        assert_eq!(family_version(DOCUMENT_MEDIA_TYPE), None);
        assert_eq!(family_version(ARTIFACT_TYPE_FAMILY_PREFIX), None);
        assert_eq!(family_version("application/vnd.cncf.notary.signature"), None);
        assert_eq!(family_version("application/vnd.saas-fabric.component.vx"), None);
    }

    #[test]
    fn the_media_type_and_api_version_share_the_artifact_types_version() {
        assert_eq!(DOCUMENT_MEDIA_TYPE, format!("{ARTIFACT_TYPE}+json"));
        assert!(API_VERSION.ends_with("/v1"));
    }
}
