//! What an environment is asked to move to.

use crate::{ReleaseUnit, Version};

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

    /// Several images moving together, as a component descriptor attached
    /// to the primary one describes them (ADR 0026 section 9).
    ///
    /// # Why not [`Unit`](Self::Unit) with the primary beside it
    ///
    /// The identity travels with the version, as for a chart: a write
    /// refuses a described release for a component whose primary is another
    /// role, and one for a component whose versions are not found this way
    /// at all. A release unit carrying an optional primary would let an
    /// images-by-role component quietly accept one.
    Described {
        /// The version, the one commit, and every image by role.
        unit: ReleaseUnit,

        /// The role whose image the component descriptor is attached to.
        primary: String,

        /// The component descriptor's manifest digest, as Fabric computed
        /// it. Named in the commit that writes this release, and never
        /// recorded in desired state: a digest typed into a platform pull
        /// request is a fact nothing proved, so every write re-applies the
        /// rule instead (ADR 0026 section 9).
        descriptor: String,
    },
}

impl Release {
    /// The version this release is.
    #[must_use]
    pub const fn version(&self) -> &Version {
        match self {
            Self::Unit(unit) | Self::Described { unit, .. } => &unit.version,
            Self::Chart { version, .. } => version,
        }
    }
}
