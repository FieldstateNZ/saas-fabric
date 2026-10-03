//! What is attached to an image digest, as far as component descriptors go.

/// The component descriptors attached to one image digest (ADR 0026
/// section 4).
///
/// # How an adapter finds them
///
/// - It asks the referrers API, `GET /v2/<name>/referrers/<digest>`. A `200`
///   with an OCI index is the list, its `Link` pages followed on the same
///   origin, bounded, and running past the bound is an error. A `200` of any
///   other content type, or a `404` whose error is not `NAME_UNKNOWN`, means
///   the registry does not serve the API. A `404 NAME_UNKNOWN` means the
///   repository does not exist, which is an error and not *nothing attached*.
/// - It **always** reads the referrers tag schema too — the tag
///   `sha256-<hex>` the publishing client maintains where the API is not
///   served — and merges the two lists by digest, so a component descriptor
///   attached before a registry started serving the API is still found. That
///   tag answering `404` means nothing is attached through it; anything but an
///   OCI index is [`Unusable::NotAnIndex`].
/// - It keeps the entries whose `artifactType` is of the component
///   descriptor family, `application/vnd.saas-fabric.component.v<N>`,
///   whatever a registry says it filtered — a registry's filter is never
///   relied on — and ignores every other referrer: a signature, an SBOM, a
///   build attestation.
/// - It fetches every candidate manifest by digest and hashes it. A `404` is
///   not attached. What remains must be an OCI image manifest of the family's
///   `artifactType`, with the empty config, exactly one layer of that type
///   plus `+json` and at most 16 KiB, and a `subject` equal to the digest
///   asked about. The tag-schema index is written by whoever publishes, so
///   none of it is trusted until each manifest has been checked.
///
/// # Why these answers and not others
///
/// **Exactly one component descriptor may be attached to a digest**, counted
/// over distinct manifest digests. More than one is refused, never chosen
/// between — a publisher's remedy is the next version — and a version Fabric
/// does not read still counts, so it is seen and named rather than skipped.
///
/// A registry that cannot be asked is never one of these answers. A rate
/// limit that read as [`Nothing`](Self::Nothing) would make a release unit
/// look undescribed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attached {
    /// Nothing of the family is attached. The domain's *undescribed*: Fabric
    /// cannot tell a publication in progress from one that will never attach
    /// a component descriptor, and says only what it saw.
    Nothing,

    /// Exactly one component descriptor is attached, and here it is.
    One(AttachedDescriptor),

    /// More than one distinct component descriptor is attached, or more
    /// candidates were listed than an adapter will fetch one by one.
    Several {
        /// The attached component descriptors' manifest digests, sorted: each
        /// one computed from bytes an adapter fetched and checked.
        ///
        /// Empty when more candidates were listed than an adapter fetches.
        /// Those were never fetched, so their digests are only what a registry
        /// or a publisher listed — and every digest Fabric records is one it
        /// computed (ADR 0026 section 3), so none is carried.
        digests: Vec<String>,
    },

    /// Something of the family is attached and cannot be used.
    Unusable {
        /// Why not.
        reason: Unusable,
    },
}

/// Why something attached could not be used — closed, so the domain decides
/// what each means rather than inheriting a string.
///
/// Each is the ADR's *invalid* with the reason *unreadable*, except as the
/// domain decides otherwise; none is *undescribed*, because something of the
/// family was plainly there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unusable {
    /// The referrers tag `sha256-<hex>` holds something other than an OCI
    /// index.
    NotAnIndex,

    /// A listed manifest names another image as its `subject`.
    OtherSubject,

    /// A listed manifest is not the shape a component descriptor's is: not
    /// an OCI image manifest, no `mediaType`, another config, not exactly one
    /// layer of the right type, a layer over 16 KiB or missing, or a digest
    /// in an algorithm Fabric does not compute.
    Malformed,
}

/// The one component descriptor attached to a digest, as published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedDescriptor {
    /// Its manifest's digest, computed from the bytes read.
    pub digest: String,

    /// Its manifest's `artifactType`: of the family, not necessarily a
    /// version Fabric reads — that is the domain's to say.
    pub artifact_type: String,

    /// Its manifest's `org.opencontainers.image.revision` annotation: the
    /// commit it was built from.
    pub revision: Option<String>,

    /// Its manifest's `org.opencontainers.image.version` annotation.
    pub version: Option<String>,

    /// Its one layer, byte for byte: bounded, hashed against the layer's
    /// digest and checked against its declared size, and never parsed here.
    pub document: Vec<u8>,
}
