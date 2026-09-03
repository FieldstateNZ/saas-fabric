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

    /// A chart, published to a chart repository.
    ///
    /// No `sourceRevision`, and the enum denies unknown fields, so one written
    /// here is refused rather than ignored. A chart repository publishes no
    /// such thing, and a field carrying a value nobody observed is worse than
    /// an absent one.
    #[serde(rename_all = "camelCase")]
    Helm {
        /// The chart repository's base URL, as the Argo source names it.
        repository: String,

        /// The chart's name within it.
        chart: String,
    },
}

impl Artifact {
    /// What this kind is called, for a message an operator reads.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::Oci { .. } => "container images",
            Self::Helm { .. } => "a Helm chart",
        }
    }
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
