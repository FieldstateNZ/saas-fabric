//! What a component is published as, and what moving it means.

use std::collections::BTreeMap;

mod release;

pub use release::Release;

/// Where a component's versions are published, in the terms discovery needs.
///
/// The domain's half of the platform repository's `artifact`. Three kinds,
/// because they are discovered differently and guarantee different things —
/// not three shapes of one thing with fields left empty.
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

    /// Container images by role, described by a component descriptor
    /// attached to the primary one (ADR 0026 section 9).
    ///
    /// The same guarantee as [`Oci`](Self::Oci) and one more: a version is
    /// eligible only when the component descriptor attached to the primary
    /// image's version tag says it is this version, names exactly these
    /// roles at these repositories, and agrees with every image on one
    /// commit — and every image still carries the version.
    Described {
        /// The role whose image carries the component descriptor, and whose
        /// repository's tags are the versions there are. One of
        /// `repositories`' roles.
        primary: String,

        /// Registry repositories by role, as the environment pins them.
        repositories: BTreeMap<String, String>,
    },
}

/// Which of the two kinds of artifact a component is published as, and
/// nothing more.
///
/// Two, although [`ArtifactSource`] has three: a described component is
/// images pinned by digest, and reports [`Oci`](Self::Oci).
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
            // Images, pinned by digest: a rollback of a described component
            // restores the same exact bytes an image rollback does, which is
            // the whole of what the console words from this. How versions
            // are *found* is not the console's concern.
            Self::Oci { .. } | Self::Described { .. } => ArtifactKind::Oci,
            Self::Helm { .. } => ArtifactKind::Helm,
        }
    }
}
