//! What a component is, and where its version is written.

use std::collections::BTreeMap;

/// Where a component's versions come from, and what provenance they carry.
///
/// # A closed set, and it stays closed
///
/// The platform has two kinds: images published to a registry, and charts
/// published to a chart repository. A third arrives by adding a variant here
/// and an implementation behind it — not by making this general enough to
/// describe one.
///
/// # Provenance lives here because it is artifact-specific
///
/// An OCI release unit carries the commit every one of its images was built
/// from, and Fabric refuses a version whose images disagree about it. A chart
/// published by somebody else carries no such thing. Making the field optional
/// on a shared struct would have let an OCI component lose its provenance and
/// stay valid — relaxed for one kind, and quietly relaxed for both. Here, an
/// `Oci` without it does not parse, and a `Helm` with it does not either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Artifact {
    /// Container images, published to a registry.
    ///
    /// Several of them, moving together: SaaS Fabric is one release unit
    /// published as three images, and a version they do not all carry is not a
    /// version this environment can run.
    #[serde(rename_all = "camelCase")]
    Oci {
        /// The commit every image was built from.
        ///
        /// Required, and required *here*: it is what makes three images one
        /// release unit rather than three that happen to share a tag.
        source_revision: String,

        /// Images by role, e.g. `runtime`, `controlPlane`, `console`.
        images: BTreeMap<String, ImagePin>,
    },
}

/// One image of a component.
///
/// No `pinnedIn`. Where a version is *written* is the component's statement
/// now, not each image's — because a chart has one version and no images, and
/// the two kinds have to answer that question in the same place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImagePin {
    /// The registry repository this image is published to.
    pub repository: String,

    /// The digest currently asked for.
    pub digest: String,
}

/// One place a component's version is written, and how.
///
/// # The renderer is the variant, not a field beside one
///
/// This is the field that would ruin the design if it grew a general escape. A
/// `jsonPath`, a regex or a YAML pointer here would turn a trusted platform
/// document into an arbitrary repository-edit engine, and Fabric into the
/// deputy that runs it — the same mistake as letting a caller name a file, one
/// level further in.
///
/// Making the renderer the *tag* rather than a field beside the others is what
/// makes an invalid combination unrepresentable instead of merely rejected. A
/// Kustomize pin that names no image does not parse; one that names a chart
/// does not either. There is no state in which a renderer is missing what it
/// needs, so no code downstream has to check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "renderer", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Pin {
    /// An `images:` entry in a Kustomize overlay, carrying a tag and a digest.
    KustomizeImage {
        /// The repository-relative file, which must sit under a managed root.
        path: String,

        /// Which image this file pins.
        ///
        /// Required, because an overlay can pin several: SaaS Fabric's control
        /// plane and console share one, and a pin that did not say which would
        /// have to guess between them.
        image: String,
    },
}

impl Pin {
    /// The file this pin is written in.
    ///
    /// Every variant has one, and every variant's is bounded by the same
    /// rules — so the check that bounds them is written once rather than per
    /// renderer.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::KustomizeImage { path, .. } => path,
        }
    }
}

/// What a component is asked to run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Desired {
    /// The version, once.
    ///
    /// Not repeated per image: several images claiming a version separately is
    /// several places for them to disagree, and disagreement is what makes a
    /// release unit incomplete rather than eligible.
    ///
    /// For a chart this is the **chart** version, which is what Argo pins. An
    /// application version is metadata beside it and is never what Fabric
    /// writes.
    pub version: String,
}
