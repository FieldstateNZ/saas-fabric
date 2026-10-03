//! The audit record for image registries (ADR 0026 section 5).
//!
//! Its own file for the reason `audit::product` is: one event, added with
//! the feature it records, rather than grown onto a file at its size limit.

use fabric_core::{event_id, EventType};

use crate::registries::RegistryHost;
use crate::DOMAIN_ID;

/// What was done to a registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryOperation {
    /// A registry was registered.
    Register,
    /// A registry was removed.
    Remove,
    /// Its credential was set or replaced.
    SetCredential,
    /// Its credential was removed.
    RemoveCredential,
    /// A repository was registered under it.
    AddRepository,
    /// A repository was removed from it.
    RemoveRepository,
}

impl RegistryOperation {
    /// The operation as the record names it.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Remove => "remove",
            Self::SetCredential => "set_credential",
            Self::RemoveCredential => "remove_credential",
            Self::AddRepository => "add_repository",
            Self::RemoveRepository => "remove_repository",
        }
    }
}

/// An operator changed a registry, or tried to and was refused.
///
/// # Refusals are recorded too
///
/// A credential a realm refused, a repository that was not readable, a
/// registration the rules turned away: each is an operator's decision that
/// did not land, and "who tried to register what, and when" is the question
/// an incident asks whether or not it worked. `outcome` is `succeeded`, or
/// the refusal's stable code.
///
/// # What is never here
///
/// A username, a token, a realm's response or a URL. The parameters are an
/// operator's subject, a validated host — `None` when the request named none
/// that could be validated — a closed operation and an outcome from a closed
/// vocabulary, so nothing that could hold one can arrive even by mistake.
pub(crate) fn registry_changed(
    operator: &str,
    host: Option<&RegistryHost>,
    operation: RegistryOperation,
    outcome: &str,
) {
    tracing::info!(
        event = "control_plane.audit.registry",
        event_id = event_id(DOMAIN_ID, EventType::Success, 14),
        operation = operation.as_str(),
        operator,
        resource = "registry",
        host = host.map_or("", RegistryHost::as_str),
        outcome,
        "operator changed an image registry"
    );
}
