//! The audit record for selecting a component version (ADR 0026 section 7).
//!
//! Its own file for the reason `audit::registry` is: one event, added with
//! the feature it records, rather than grown onto a file at its size limit.

use fabric_core::{event_id, EventType};

use crate::{ControlPlaneError, Operator, DOMAIN_ID};

/// What an operator asked to select.
pub(crate) struct Selecting<'a> {
    /// `<application>/<component>`, as the catalogue's activity names it.
    pub(crate) entry: &'a str,

    /// The primary image's repository.
    pub(crate) repository: &'a str,

    /// The version tag.
    pub(crate) version: &'a str,
}

/// How a selection turned out.
pub(crate) enum SelectionOutcome<'a> {
    /// It was resolved and written.
    Selected {
        /// The digest the version tag resolved to, which Fabric computed.
        primary_digest: &'a str,

        /// The component descriptor's digest, which Fabric computed.
        descriptor_digest: &'a str,
    },

    /// It was refused, with this.
    Refused(&'a ControlPlaneError),
}

/// An operator selected a component version, or tried to and was refused.
///
/// # What it names
///
/// The operator, the outcome, the repository, the version, and for a
/// selection written, the primary image's digest and the component
/// descriptor's digest — what was proven, not what was asked for. A refusal
/// is recorded too, as `outcome` set to its stable code and, when the rule
/// answered, `answer` and `reason`: "who tried to select what, and what
/// Fabric saw" is the question an incident asks whether or not it landed.
/// A written selection is also a catalogue change, recorded as every one is
/// with the revision it made.
///
/// # What is never here
///
/// A credential, a registry's response or a URL: every value is a validated
/// name, a digest Fabric computed, or a code from a closed vocabulary.
pub(crate) fn component_selected(
    operator: &Operator,
    selecting: &Selecting<'_>,
    outcome: &SelectionOutcome<'_>,
) {
    let (outcome, answer, reason, primary_digest, descriptor_digest) = match outcome {
        SelectionOutcome::Selected {
            primary_digest,
            descriptor_digest,
        } => ("selected", "", "", *primary_digest, *descriptor_digest),
        SelectionOutcome::Refused(error) => {
            let (answer, reason) = match error {
                ControlPlaneError::Selection(refusal) => refusal.answer().unwrap_or(("", None)),
                _ => ("", None),
            };
            (error.code(), answer, reason.unwrap_or(""), "", "")
        }
    };
    tracing::info!(
        event = "control_plane.audit.component_selected",
        event_id = event_id(DOMAIN_ID, EventType::Success, 15),
        operation = "select_component_version",
        requested_by = operator.subject(),
        resource = "catalogue",
        entry = selecting.entry,
        repository = selecting.repository,
        version = selecting.version,
        outcome,
        answer,
        reason,
        primary_digest,
        descriptor_digest,
        "operator selected a component version"
    );
}
