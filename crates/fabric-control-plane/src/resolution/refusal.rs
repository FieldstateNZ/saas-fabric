//! Why a component version was not selected, apart from a registry that
//! could not be asked.
//!
//! In the 121–150 line band: the refusals and the rule's answers are one
//! closed vocabulary, each variant's wording beside it.

use std::fmt;

use fabric_client_model::ClientId;
use fabric_platform_management::InvalidReason;

/// A refused selection (ADR 0026 section 7).
///
/// A registry that could not be asked is not here: it is the registry's own
/// failure, `registry_unavailable` or `registry_refused`, and never an
/// answer — a rate limit must not read as *undescribed*. Statuses and codes,
/// with the reason for each, are in `errors::status_mapping::selection`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectionRefusal {
    /// The primary repository is not registered under any registry, so it
    /// was refused before any registry was asked.
    #[error(
        "{repository} is not registered under any registry; register it before selecting a version from it"
    )]
    RepositoryNotRegistered {
        /// The repository.
        repository: String,
    },

    /// The primary repository has no such tag.
    #[error("{repository} has no tag {version}")]
    VersionNotFound {
        /// The repository.
        repository: String,
        /// The version tag asked for.
        version: String,
    },

    /// The tag exists, and the rule did not call it a release unit.
    #[error("{repository} {version} cannot be selected: {answer}")]
    Unusable {
        /// The repository.
        repository: String,
        /// The version tag.
        version: String,
        /// What the rule answered.
        answer: Unusable,
    },

    /// The version resolved to the component descriptor the component
    /// already records, so nothing was written.
    #[error(
        "component '{component}' of '{application}' already records the component descriptor {descriptor_digest}"
    )]
    AlreadySelected {
        /// The application.
        application: ClientId,
        /// The component.
        component: ClientId,
        /// The component descriptor it records.
        descriptor_digest: String,
    },

    /// The component is a platform capability, which is operator-authored
    /// and has no version to select.
    #[error("component '{component}' of '{application}' is a platform capability; a version cannot be selected for it")]
    CapabilityNotSelectable {
        /// The application.
        application: ClientId,
        /// The component.
        component: ClientId,
    },
}

/// What the rule answered for a version that is not a release unit — each
/// its own wording in the console, and none falling through to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unusable {
    /// No component descriptor is attached to the digest the tag resolves
    /// to. Fabric cannot tell a publication in progress from one that will
    /// never attach one, and says only what it saw.
    Undescribed,

    /// The images and the component descriptor name different commits, or
    /// another image's version tag is missing or points at other bytes.
    Incoherent,

    /// A component descriptor is present and cannot be used, for a reason
    /// from the rule's closed list.
    Invalid(InvalidReason),
}

impl Unusable {
    /// The answer's stable spelling, for the console to key its wording by.
    #[must_use]
    pub const fn answer(&self) -> &'static str {
        match self {
            Self::Undescribed => "undescribed",
            Self::Incoherent => "incoherent",
            Self::Invalid(_) => "invalid",
        }
    }

    /// For *invalid*, the reason's stable code; nothing for the others.
    #[must_use]
    pub const fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Undescribed | Self::Incoherent => None,
            Self::Invalid(reason) => Some(reason.code()),
        }
    }
}

impl fmt::Display for Unusable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undescribed => {
                formatter.write_str("no component descriptor is attached to the image its tag resolves to")
            }
            Self::Incoherent => formatter.write_str(
                "its images and component descriptor name different commits, or an image's version tag \
                 does not point at the bytes the component descriptor names",
            ),
            Self::Invalid(reason) => write!(formatter, "its component descriptor is invalid: {reason}"),
        }
    }
}
