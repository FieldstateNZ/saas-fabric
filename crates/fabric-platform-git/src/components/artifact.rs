//! What a component is published as, and what version grammar applies.

use std::collections::BTreeMap;

use fabric_platform_management::Version;

use crate::components::ImagePin;

/// The words this crate uses for the image shape, shared so that
/// `Artifact::describe` and `WantedVersion::describe` — two names for the
/// same release shape, read together in one refusal message — cannot drift
/// into different wording.
pub(crate) const IMAGES_WORDS: &str = "container images";

/// The words this crate uses for the Helm shape. See [`IMAGES_WORDS`].
pub(crate) const HELM_WORDS: &str = "a Helm chart";

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
            Self::Oci { .. } => IMAGES_WORDS,
            Self::Helm { .. } => HELM_WORDS,
        }
    }

    /// Parses a version in this kind's grammar.
    ///
    /// Which grammar applies is a fact about what a component *is*, not
    /// about the text in front of it. An OCI tag cannot carry `+` — it is
    /// not a legal tag character — so an image's desired version is refused
    /// if it carries build metadata, the same rule [`Version::parse`]
    /// applies everywhere else in this platform. A Helm chart is not an
    /// image: chart repositories publish build metadata routinely, and Argo
    /// pins whatever string the index carried, so a chart's desired version
    /// is read with [`Version::parse_chart`], which keeps it.
    ///
    /// Dispatching on the artifact here, rather than fixing one grammar at
    /// the call site, is what lets a chart version discovered *with* build
    /// metadata be advanced to and then read back. A single global parser
    /// could write such a version — `advance` never asked what kind it was —
    /// but would then refuse to read the very thing it had just written.
    #[must_use]
    pub fn parse_version(&self, text: &str) -> Option<Version> {
        match self {
            Self::Oci { .. } => Version::parse(text),
            Self::Helm { .. } => Version::parse_chart(text),
        }
    }
}
