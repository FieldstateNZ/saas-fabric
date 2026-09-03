//! What a component is published as, and what moving it means.

use std::collections::BTreeMap;

use crate::{ReleaseUnit, Version};

/// Where a component's versions are published, in the terms discovery needs.
///
/// The domain's half of the platform repository's `artifact`. Two kinds,
/// because they are discovered differently and guarantee different things —
/// not two shapes of one thing with fields left empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSource {
    /// Container images by role, published to a registry.
    ///
    /// A version is eligible only when every image carries it and they agree
    /// on the commit they were built from, and what gets deployed is an
    /// immutable digest.
    Oci {
        /// Registry repositories by role.
        repositories: BTreeMap<String, String>,
    },

    /// A chart, published to a chart repository.
    ///
    /// # A weaker guarantee, and it is named rather than hidden
    ///
    /// A classic chart repository pins a *version*, not a digest. The bytes
    /// behind `7.3.0` can be republished, and nothing here would see it. That
    /// is strictly weaker than what the OCI kind gives, and it is why some
    /// operations this platform offers for images are refused for charts —
    /// see [`Rollback`](crate::PlatformError::RollbackUnsupported).
    Helm {
        /// The chart repository's base URL.
        repository: String,

        /// The chart's name within it.
        chart: String,
    },
}

impl ArtifactSource {
    /// What this kind is called, for a message an operator reads.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::Oci { .. } => "container images",
            Self::Helm { .. } => "a Helm chart",
        }
    }
}

/// What an environment is asked to move to.
///
/// Kept separate from [`ReleaseUnit`] rather than widening it. A release unit
/// is the OCI concept — one version published as several images that agree on
/// their source — and a chart version is not a degenerate one of those. Making
/// it a variant keeps the vocabulary meaning what it has always meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Release {
    /// Several images, moving together.
    Unit(ReleaseUnit),

    /// A chart version, and the chart it is a version of.
    ///
    /// # Why the identity travels with the version
    ///
    /// A bare version says nothing about *what* it is a version of. Discovery
    /// found `7.3.1` of one chart in one repository; a pin names a chart in a
    /// repository too, and if the write does not compare them then a release
    /// discovered from one chart can be written into a pin for another. The
    /// number would be plausible and the software would be wrong.
    Chart {
        /// The chart repository this version was discovered in.
        repository: String,

        /// The chart it is a version of.
        chart: String,

        /// The chart version. Not the application version: Argo pins the
        /// chart, and an application version is metadata beside it.
        version: Version,
    },
}

impl Release {
    /// The version this release is.
    #[must_use]
    pub const fn version(&self) -> &Version {
        match self {
            Self::Unit(unit) => &unit.version,
            Self::Chart { version, .. } => version,
        }
    }
}
