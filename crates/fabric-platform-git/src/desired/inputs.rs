//! What a caller asks a desired-state write to do.

use std::collections::BTreeMap;

/// One image's new identity, as the caller resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDigest {
    /// Where the image was published. Checked against what the manifest
    /// already declares for this role, and refused if it disagrees — a caller
    /// may move a component to a new *version*, never to a new registry.
    pub repository: String,

    /// The immutable digest to deploy.
    pub digest: String,
}

/// What a caller asks a component to move to.
///
/// A release unit: one version, one source commit, and every image the
/// component publishes. Not a per-image update — promoting the console without
/// the control plane would put two thirds of a release on an environment and
/// call it integrated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentVersion {
    /// The version, once.
    pub version: String,

    /// The commit every image was built from.
    pub source_revision: String,

    /// Images by role. Must be exactly the roles the manifest declares.
    pub images: BTreeMap<String, ImageDigest>,
}

/// What a caller asks a component to move to.
///
/// Two shapes, because the two artifact kinds carry different things and
/// neither is a degenerate case of the other. A chart version has no digests
/// to travel with it and no source commit to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WantedVersion {
    /// Several images, moving together.
    Images(ComponentVersion),

    /// A chart version, and the chart it is a version of.
    ///
    /// The identity travels with the version so that the write can refuse a
    /// release discovered somewhere other than where the manifest says to put
    /// it. A number alone would be plausible against the wrong chart.
    Chart {
        /// The chart repository it was discovered in.
        repository: String,

        /// The chart it is a version of.
        chart: String,

        /// The chart version, as it is published.
        version: String,
    },
}

impl WantedVersion {
    /// The version this asks for.
    #[must_use]
    pub fn version(&self) -> &str {
        match self {
            Self::Images(unit) => &unit.version,
            Self::Chart { version, .. } => version,
        }
    }

    /// What shape this request is, for a message an operator reads.
    ///
    /// Worded to match [`Artifact::describe`](crate::components::Artifact::describe),
    /// so a refusal reads as "publishes X, and the request carries Y" rather
    /// than two unrelated vocabularies describing the same disagreement.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::Images(_) => "container images",
            Self::Chart { .. } => "a Helm chart",
        }
    }
}
