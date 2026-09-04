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
    /// is strictly weaker than what the OCI kind gives. It is not a reason to
    /// refuse an operation: rolling back restores an older published
    /// version, and for a chart the version is what there is to restore. The difference is *stated* to the operator — see
    /// [`ArtifactKind`] — rather than enforced by declining to act.
    Helm {
        /// The chart repository's base URL.
        repository: String,

        /// The chart's name within it.
        chart: String,
    },
}

/// Which of the two kinds a component is published as, and nothing more.
///
/// [`ArtifactSource`] carries *where* things are published, which is what
/// discovery needs and what nobody outside this crate should have to hold.
/// This carries only which kind it is, because that is the whole of what the
/// console needs in order to word what a rollback of this component restores.
///
/// # Why the console is told the kind rather than a yes-or-no
///
/// It used to be told `rollable: true/false`, and the answer for a chart was
/// `false`. Now both kinds can be rolled back and the halves of the guarantee
/// differ — an image rollback restores the exact bytes, a chart rollback
/// restores the version — so the console needs to say *which*, and a boolean
/// has nowhere to say it from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// Container images, published to a registry.
    Oci,

    /// A chart, published to a chart repository.
    Helm,
}

impl ArtifactSource {
    /// Which kind this is, without where it is published.
    #[must_use]
    pub const fn kind(&self) -> ArtifactKind {
        match self {
            Self::Oci { .. } => ArtifactKind::Oci,
            Self::Helm { .. } => ArtifactKind::Helm,
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
