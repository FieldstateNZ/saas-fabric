//! What a component is published as, and what version grammar applies.

use std::collections::BTreeMap;

use crate::components::ImagePin;

mod grammar;

/// The words this crate uses for the image shape, shared so that
/// `Artifact::describe` and `WantedVersion::describe` — two names for the
/// same release shape, read together in one refusal message — cannot drift
/// into different wording.
pub(crate) const IMAGES_WORDS: &str = "container images";

/// The words this crate uses for the Helm shape. See [`IMAGES_WORDS`].
pub(crate) const HELM_WORDS: &str = "a Helm chart";

/// The words this crate uses for the described shape. See [`IMAGES_WORDS`].
pub(crate) const DESCRIBED_WORDS: &str = "container images a component descriptor describes";

/// Where a component's versions come from, and what provenance they carry.
///
/// # A closed set, and it stays closed
///
/// The platform has three kinds: images published to a registry, charts
/// published to a chart repository, and images a component descriptor
/// describes (schema 3, ADR 0026 section 9). Another arrives by adding a
/// variant here and an implementation behind it — not by making this general
/// enough to describe one.
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

    /// Container images, found through the component descriptor attached to
    /// the primary one (ADR 0026 section 9). Schema 3 only.
    ///
    /// The images and the commit stay, exactly as for [`Oci`](Self::Oci):
    /// the overlays are rewritten from them, and the platform's own check
    /// compares every rendered reference with them. The component
    /// descriptor's digest is **not** recorded — a digest typed into a
    /// platform pull request is a fact nothing proved — so every advance and
    /// rollback re-applies the rule, and its commit message names it.
    #[serde(rename_all = "camelCase")]
    Described {
        /// The role whose image carries the component descriptor. One of
        /// `images`, or the document is refused.
        primary: String,

        /// The commit every image and the component descriptor were built
        /// from.
        source_revision: String,

        /// Images by role.
        images: BTreeMap<String, ImagePin>,
    },
}
