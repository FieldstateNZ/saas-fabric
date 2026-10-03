//! The component descriptor: a small JSON document, attached to a
//! component's primary image as an OCI artifact, that says what the
//! component is (ADR 0026 sections 1 and 2).
#[macro_use]
mod newtype;
mod component_name;
mod constants;
#[cfg(test)]
mod descriptor_tests;
mod digest;
mod envelope;
mod format_characters;
#[cfg(test)]
mod newtypes_tests;
mod read;
mod repository;
mod role;
mod source;
mod spec;
mod strict_json;
#[cfg(test)]
mod v1_shape_tests;
mod validate;
mod version;
pub use component_name::ComponentName;
pub use constants::{
    family_version, API_VERSION, ARTIFACT_TYPE, ARTIFACT_TYPE_FAMILY_PREFIX, DOCUMENT_FILE_NAME,
    DOCUMENT_MEDIA_TYPE, KIND, MAX_DOCUMENT_BYTES, MAX_FIELDS, MAX_IMAGES, MAX_RESOURCES,
};
pub use digest::Digest;
pub use repository::Repository;
pub use role::Role;
pub use source::{ComponentSource, ComponentSourceSpec, SourceImage};
pub use spec::{ComponentSpec, ImageReference};
pub use version::ComponentVersion;

use crate::ContractError;

/// A valid v1 component descriptor.
///
/// One exists only once its `spec` has passed [`validate`](Self::validate):
/// through [`from_json`](Self::from_json) or
/// [`from_artifact`](Self::from_artifact) when reading one, through
/// [`ComponentSource::render`] when publishing one, or through
/// [`new`](Self::new).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDescriptor {
    /// What the component is.
    spec: ComponentSpec,
}

impl ComponentDescriptor {
    /// Accepts `spec` once it is valid.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::Invalid`] for the first rule it breaks.
    pub fn new(spec: ComponentSpec) -> Result<Self, ContractError> {
        let descriptor = Self { spec };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Checks every rule of ADR 0026 section 2 that the types do not already
    /// hold: the title is required text of at most 128 bytes and the
    /// description at most 2048; one to [`MAX_IMAGES`] images, all on one
    /// registry, no repository named by two roles; capabilities unique; at
    /// most [`MAX_FIELDS`] fields and [`MAX_RESOURCES`] resources, each valid
    /// by the catalogue's own rules; no Unicode format character in any text
    /// it declares; and canonical bytes of at most [`MAX_DOCUMENT_BYTES`], so
    /// that [`to_json`](Self::to_json) always writes a document
    /// [`from_json`](Self::from_json) reads.
    ///
    /// # Why one repository has one role
    ///
    /// A reader finds each image by the version tag in its repository (ADR
    /// 0026 section 3), and one repository has one tag of a version, so two
    /// roles there can only name one image twice -- and
    /// [`role_of`](Self::role_of) could not say which role it is.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::Invalid`] for the first rule broken.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate::validate(&self.spec)
    }

    /// The canonical bytes: compact JSON, the envelope first, every field in
    /// its declared order, and every map sorted -- so equal descriptors have
    /// equal bytes, and [`from_json`](Self::from_json) reads them back equal.
    ///
    /// # Why this cannot fail
    ///
    /// Every key is a string and every value a string, boolean, list or
    /// struct, which is everything `serde_json` writes without error.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(&envelope::Written::wrapping(&self.spec))
            .unwrap_or_else(|error| unreachable!("a component descriptor always serializes: {error}"))
    }

    /// What the component is.
    #[must_use]
    pub fn spec(&self) -> &ComponentSpec {
        &self.spec
    }

    /// The role of the image at exactly `repository` and `digest`, if this
    /// descriptor names one.
    #[must_use]
    pub fn role_of(&self, repository: &Repository, digest: &Digest) -> Option<&Role> {
        self.spec
            .images
            .iter()
            .find(|(_, image)| &image.repository == repository && &image.digest == digest)
            .map(|(role, _)| role)
    }
}
