//! The media types this client asks for and checks.

/// An OCI image manifest.
pub(super) const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";

/// An OCI index: a multi-platform image, and a referrers list.
pub(super) const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";

/// The empty config an OCI artifact carries.
pub(super) const EMPTY_CONFIG: &str = "application/vnd.oci.empty.v1+json";

/// Both the OCI and the older Docker media types, image and index — what a
/// manifest is asked for as.
pub(super) const MANIFEST_TYPES: &str = "application/vnd.oci.image.manifest.v1+json, \
     application/vnd.docker.distribution.manifest.v2+json, \
     application/vnd.oci.image.index.v1+json, \
     application/vnd.docker.distribution.manifest.list.v2+json";

/// Whether `response` says its body is of `expected` media type, parameters
/// such as a charset ignored.
pub(super) fn is(response: &reqwest::Response, expected: &str) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case(expected))
}
