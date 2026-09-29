//! What a kind is called, and which version grammar it reads.

use fabric_platform_management::Version;

use super::{Artifact, DESCRIBED_WORDS, HELM_WORDS, IMAGES_WORDS};

impl Artifact {
    /// What this kind is called, for a message an operator reads.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::Oci { .. } => IMAGES_WORDS,
            Self::Helm { .. } => HELM_WORDS,
            Self::Described { .. } => DESCRIBED_WORDS,
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
            // Both are image tags, and an image tag carries no `+`.
            Self::Oci { .. } | Self::Described { .. } => Version::parse(text),
            Self::Helm { .. } => Version::parse_chart(text),
        }
    }
}
