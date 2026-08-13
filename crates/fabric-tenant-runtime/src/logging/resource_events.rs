//! Log events about one named resource, not about the registry as a whole.

use fabric_core::{event_id, BindingRevision, EventType};

use crate::resource::RegistryResource;
use crate::{ConfigurationError, DOMAIN_ID};

/// An incoming resource was older than the one already held.
///
/// Worth noticing: it usually means two sources are publishing, or one is
/// reading a replica that has fallen behind.
pub(crate) fn stale_resource_ignored<T: RegistryResource>(
    key: &T::Key,
    incoming: BindingRevision,
    held: BindingRevision,
) {
    tracing::warn!(
        event = "runtime.stale_resource_ignored",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        resource_kind = T::KIND,
        resource_key = %key,
        incoming_revision = incoming.get(),
        held_revision = held.get(),
        "ignoring a resource older than the one held"
    );
}

/// An incoming resource matched the revision already held, but not the
/// payload.
///
/// See [`ApplyReport::divergent_payload`](crate::ApplyReport::divergent_payload)
/// for why this is rejected rather than accepted: the revision is the single
/// source of truth for "did this resource change", so a payload that disagrees
/// with it is a reconciliation bug — most likely a real edit that forgot to
/// bump the revision — and is surfaced here rather than silently winning or
/// silently losing.
pub(crate) fn divergent_payload_at_same_revision<T: RegistryResource>(
    key: &T::Key,
    revision: BindingRevision,
) {
    tracing::warn!(
        event = "runtime.divergent_payload_at_same_revision",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 4),
        resource_kind = T::KIND,
        resource_key = %key,
        revision = revision.get(),
        "ignoring a resource whose payload differs from what is held at the same revision; \
         the revision was not bumped, so the payload change was not applied"
    );
}

/// A resource failed validation and was never installed.
///
/// Error rather than warning, unlike the two above. A stale revision or a
/// divergent payload corrects itself the moment the source publishes again;
/// this one does not. The resource stays unusable — every request touching it
/// fails — until a human fixes whatever reconciled it, so it needs to reach
/// somebody rather than sit in a warning stream.
pub(crate) fn invalid_resource_rejected<T: RegistryResource>(key: &T::Key, error: &ConfigurationError) {
    tracing::error!(
        event = "runtime.invalid_resource_rejected",
        event_id = event_id(DOMAIN_ID, EventType::Error, 2),
        resource_kind = T::KIND,
        resource_key = %key,
        reason = %error,
        "rejected a resource that failed validation; it was not installed, and \
         any copy already held is still being served"
    );
}
