//! What a SaaS Fabric component says it is: the component descriptor, its
//! renderer, and the configuration-field, resource and capability shapes it
//! shares with the catalogue (ADR 0026).
//!
//! A **component descriptor** -- never an OCI descriptor -- is a small JSON
//! document, attached to a component's primary image as an OCI artifact,
//! naming the component, its version, every image by role and digest, the
//! platform capabilities it needs, and the configuration fields and Data API
//! resources it declares. A component's repository holds the authored form,
//! `component.yaml`, and its release renders the published form from it with
//! [`ComponentSource::render`].
//!
//! # Why this crate is in neither plane
//!
//! Two readers need these shapes: the catalogue in `fabric-client-model`,
//! which is control plane, and Platform Management, which will read a
//! component descriptor to decide what a version is and is in neither plane. A shape
//! either one owned would put the other on the wrong side of an edge. So
//! this crate depends on `fabric-core` and `fabric-runtime-publication` and
//! nothing else, on the same footing as the latter, and anything outside the
//! runtime plane may depend on it: its edge to `fabric-runtime-publication`
//! puts it behind ADR 0018's publisher fence, which no runtime-plane crate
//! may cross.
//!
//! # One declaration of each shape
//!
//! [`ConfigurationField`], [`FieldKind`] and [`ApplicationResource`], with
//! their validators, moved here from `fabric-client-model`, which re-exports
//! them at their old paths byte for byte. A component descriptor declares
//! fields and resources in the catalogue's own shape, checked by the
//! catalogue's own rules -- not a second copy that could drift from the
//! first -- and [`PlatformCapability`] is ADR 0021's closed list as a type
//! both use.
//!
//! # Why there is no transport
//!
//! Nothing here reads a registry. Finding an attached component descriptor,
//! fetching and hashing it, is an adapter's job; this crate only says whether
//! bytes already in hand are one, and renders them. That keeps it a domain
//! crate, checked as one: a release can run its renderer, and a test can
//! read one, with no network at all.
mod capability;
mod descriptor;
mod errors;
mod fields;
mod resource;
mod validation;

pub use capability::PlatformCapability;
pub use descriptor::{
    family_version, ComponentDescriptor, ComponentName, ComponentSource, ComponentSourceSpec, ComponentSpec,
    ComponentVersion, Digest, ImageReference, Repository, Role, SourceImage, API_VERSION, ARTIFACT_TYPE,
    ARTIFACT_TYPE_FAMILY_PREFIX, DOCUMENT_FILE_NAME, DOCUMENT_MEDIA_TYPE, KIND, MAX_DOCUMENT_BYTES,
    MAX_FIELDS, MAX_IMAGES, MAX_RESOURCES,
};
pub use errors::ContractError;
pub use fields::{ConfigurationField, FieldKind};
pub use resource::ApplicationResource;
pub use validation::{
    check_key, check_revision, check_value, is_hostname, is_identifier, is_timezone, text, unique,
    validate_fields, validate_resources, MAX_REVISION_BYTES,
};
