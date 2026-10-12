//! Log events about the persisted last-known-good copy.
//!
//! No metrics exporter exists in this workspace, so these events, with
//! their stable IDs, and the readiness detail are how a hold is observed.

use fabric_core::{event_id, EventType};

use crate::publication::LastKnownGoodRefusal;
use crate::DOMAIN_ID;

/// A validated published generation was written to the copy.
pub(crate) fn last_known_good_persisted(path: &str, generation: u64) {
    tracing::info!(
        event = "runtime.last_known_good_persisted",
        event_id = event_id(DOMAIN_ID, EventType::Success, 6),
        path,
        generation,
        "persisted the last complete generation"
    );
}

/// A published generation above the copy replaced it at start.
pub(crate) fn last_known_good_superseded(path: &str, stored: u64, published: u64) {
    tracing::info!(
        event = "runtime.last_known_good_superseded",
        event_id = event_id(DOMAIN_ID, EventType::Success, 7),
        path,
        stored,
        published,
        "a newer published generation validated; the last-known-good copy is superseded"
    );
}

/// The replica primed from the copy, not from the published files. An
/// error, not a warning: the replica is serving a held generation until a
/// published one at or above it validates, and that needs a look.
pub(crate) fn last_known_good_served(path: &str, generation: u64, held_because: &str) {
    tracing::error!(
        event = "runtime.last_known_good_served",
        event_id = event_id(DOMAIN_ID, EventType::Error, 8),
        path,
        generation,
        held_because,
        "no published generation could be applied at start; serving the last-known-good generation"
    );
}

/// The copy exists and was refused, so it is not served.
pub(crate) fn last_known_good_refused(path: &str, refusal: &LastKnownGoodRefusal) {
    tracing::error!(
        event = "runtime.last_known_good_refused",
        event_id = event_id(DOMAIN_ID, EventType::Error, 9),
        path,
        reason = %refusal,
        "the last-known-good copy was refused and will not be served"
    );
}

/// A generation could not be written to the copy. The copy on disk, if
/// any, is whatever an earlier write left whole.
pub(crate) fn last_known_good_persist_failed(path: &str, generation: u64, error: &std::io::Error) {
    tracing::error!(
        event = "runtime.last_known_good_persist_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 10),
        path,
        generation,
        reason = %error,
        "could not persist the last complete generation"
    );
}
