//! Why a component descriptor that is present cannot be used: a closed list.
//!
//! Over the 120-line advisory threshold. The reason is that the list is one
//! closed vocabulary with its stable codes and its operator wording beside
//! each variant, and splitting the three would let a variant gain one
//! without the others.

use std::fmt;

/// Why a version is *invalid* (ADR 0026 section 3): a component descriptor
/// is present and cannot be used.
///
/// # Why closed, and why each has a code
///
/// The console words each reason for itself, and a reason it had never been
/// told about would fall through to another's wording — the UI stating what
/// Fabric did not observe. So the list is an enum and not a string, a new
/// reason is a new variant every match has to consider, and
/// [`code`](Self::code) is the stable spelling a surface keys its wording
/// by, which never changes once published.
///
/// None of these is *undescribed*: in every case something of the component
/// descriptor family was plainly there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidReason {
    /// The component descriptor could not be read: not the shape of one, a
    /// document that breaks a rule of its version, or an attached artifact
    /// the registry adapter found unusable.
    Unreadable,

    /// A component descriptor of a version this build does not read.
    UnsupportedVersion {
        /// The version it named, such as `v2`.
        found: String,
    },

    /// The component descriptor names a version other than the tag it was
    /// found by, or its `version` annotation does. One digest has one
    /// version.
    WrongVersion,

    /// More than one component descriptor is attached to the digest. Fabric
    /// never chooses between them; a publisher's remedy is the next version.
    Several,

    /// The component descriptor does not name the image it is attached to,
    /// at that repository and digest, as one of its images.
    PrimaryNotNamed,

    /// The component descriptor names images on more than one registry, so
    /// one of them is not on the primary image's. Found when the document is
    /// read, since its version's rules refuse it.
    OtherRegistry,

    /// An image the component descriptor names does not exist at its digest.
    MissingImage {
        /// The role it names that image by.
        role: String,
    },

    /// Something that must say which commit it was built from says none, or
    /// more than one.
    NoSingleRevision {
        /// Which thing.
        of: RevisionOf,
    },

    /// The component descriptor names a repository no registry holds
    /// registered (the catalogue's rule).
    NotRegistered {
        /// The first such repository, in role order.
        repository: String,
    },

    /// The component descriptor's roles, their repositories or its primary
    /// differ from the ones the environment pins (Platform Management's
    /// rule): a self-consistent release, and not one this environment's pins
    /// can take.
    NotPinned,
}

/// What a [`InvalidReason::NoSingleRevision`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionOf {
    /// One of the component's images, by role.
    Image {
        /// The role.
        role: String,
    },

    /// The component descriptor itself, whose `revision` annotation is
    /// required.
    ComponentDescriptor,
}

impl InvalidReason {
    /// The reason's stable spelling, for a surface to key its wording by.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::UnsupportedVersion { .. } => "unsupportedVersion",
            Self::WrongVersion => "wrongVersion",
            Self::Several => "several",
            Self::PrimaryNotNamed => "primaryNotNamed",
            Self::OtherRegistry => "otherRegistry",
            Self::MissingImage { .. } => "missingImage",
            Self::NoSingleRevision { .. } => "noSingleRevision",
            Self::NotRegistered { .. } => "notRegistered",
            Self::NotPinned => "notPinned",
        }
    }
}

impl fmt::Display for InvalidReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable => formatter.write_str("the component descriptor could not be read"),
            Self::UnsupportedVersion { found } => write!(
                formatter,
                "the component descriptor is of version {found}, which this build does not read"
            ),
            Self::WrongVersion => {
                formatter.write_str("the component descriptor names a version other than its tag")
            }
            Self::Several => formatter.write_str("more than one component descriptor is attached"),
            Self::PrimaryNotNamed => {
                formatter.write_str("the component descriptor does not name the image it is attached to")
            }
            Self::OtherRegistry => formatter.write_str("the component descriptor names images on another registry"),
            Self::MissingImage { role } => write!(formatter, "the {role} image does not exist at its digest"),
            Self::NoSingleRevision {
                of: RevisionOf::Image { role },
            } => write!(formatter, "the {role} image names no single revision"),
            Self::NoSingleRevision {
                of: RevisionOf::ComponentDescriptor,
            } => formatter.write_str("the component descriptor names no revision"),
            Self::NotRegistered { repository } => write!(formatter, "{repository} is not registered"),
            Self::NotPinned => formatter.write_str(
                "the component descriptor's roles, repositories or primary are not the ones this environment pins",
            ),
        }
    }
}
