//! What the process observed about registries, as opposed to what an
//! operator decided — that is [`audit::registry_changed`](crate::audit).
//!
//! Beside the module rather than in `logging` for the reason
//! `logging::integration` is its own file: one cohesive group, added together.
//! The same rules hold: no credential, no username, no realm response and no
//! URL reaches any of these; a host, which names a record, and this
//! platform's own sentence do.

use fabric_core::{event_id, EventType};

use crate::registries::RegistryHost;
use crate::DOMAIN_ID;

/// A registry could not be restored as it was recorded, at startup.
///
/// Warning, and never fatal: the console that exists to fix it must still
/// load. `host` is `None` when the record set itself could not be read.
pub(super) fn restore_incomplete(host: Option<&RegistryHost>, detail: &str) {
    tracing::warn!(
        event = "control_plane.registry_restore_incomplete",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 6),
        host = host.map_or("", RegistryHost::as_str),
        detail,
        "a registry was not restored as it was recorded"
    );
}

/// A credential that no record names any more could not be deleted.
///
/// A change that replaced or removed it has already been recorded, so the
/// change stands; what is left is an unreferenced token in the secret
/// partition, named here by the registry it belonged to so an operator can
/// see there is something to clear.
pub(super) fn credential_left_behind(host: &RegistryHost) {
    tracing::warn!(
        event = "control_plane.registry_credential_left_behind",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 7),
        host = host.as_str(),
        "a replaced or removed registry credential could not be deleted from the secret store"
    );
}
