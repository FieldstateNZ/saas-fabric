//! Which status, and which stable code, a refused selection carries (ADR
//! 0026 section 7).
//!
//! | Code | Status | When |
//! |---|---|---|
//! | `repository_not_registered` | 422 | the primary repository is not registered; no registry was asked |
//! | `component_version_not_found` | 422 | the repository has no such tag |
//! | `component_version_unusable` | 422 | the rule answered *undescribed*, *incoherent* or *invalid*; the body carries `answer`, and `reason` for *invalid* |
//! | `component_version_already_selected` | 409 | the component already records that component descriptor |
//! | `capability_not_selectable` | 422 | the component is a platform capability |
//!
//! A registry that could not be asked is the registry's own
//! `registry_unavailable` (`503`, `Retry-After`) — a deadline reached
//! included — and a refused request or credential its `registry_refused`
//! (`502`).

use http::StatusCode;

use crate::SelectionRefusal;

impl SelectionRefusal {
    /// The status an operator's browser sees.
    #[allow(
        clippy::match_same_arms,
        reason = "arms are grouped by cause; the code keeps each apart"
    )]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            // Understood, and what it names cannot be selected: register the
            // repository, pick another version, or publish one that is a
            // release unit. Not a 404 — the route exists — and not a 502:
            // the registry answered, and its answer is the refusal.
            Self::RepositoryNotRegistered { .. } | Self::VersionNotFound { .. } | Self::Unusable { .. } => {
                StatusCode::UNPROCESSABLE_ENTITY
            }

            // A capability is operator-authored; there is nothing to resolve.
            Self::CapabilityNotSelectable { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // What is there already is what the request asked for, so
            // nothing is written and a timestamp never becomes a change.
            Self::AlreadySelected { .. } => StatusCode::CONFLICT,
        }
    }

    /// A stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::RepositoryNotRegistered { .. } => "repository_not_registered",
            Self::VersionNotFound { .. } => "component_version_not_found",
            Self::Unusable { .. } => "component_version_unusable",
            Self::AlreadySelected { .. } => "component_version_already_selected",
            Self::CapabilityNotSelectable { .. } => "capability_not_selectable",
        }
    }

    /// The rule's answer, and for *invalid* its reason's code, for the body
    /// of `component_version_unusable`: the console words each answer for
    /// itself, and the message alone would leave it parsing prose.
    #[must_use]
    pub const fn answer(&self) -> Option<(&'static str, Option<&'static str>)> {
        match self {
            Self::Unusable { answer, .. } => Some((answer.answer(), answer.reason())),
            Self::RepositoryNotRegistered { .. }
            | Self::VersionNotFound { .. }
            | Self::AlreadySelected { .. }
            | Self::CapabilityNotSelectable { .. } => None,
        }
    }
}
