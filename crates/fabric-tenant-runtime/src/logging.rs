//! Structured log events for the runtime plane.
//!
//! The registry lifecycle is generic over the resource type, so these helpers
//! are too: `T::KIND` supplies the `resource_kind` field, which is what lets a
//! single set of events cover both tenant bindings and data sources without
//! either losing its identity in the logs.
//!
//! Split along the axis an on-call reader cares about. [`resource_events`]
//! covers *one named resource* going wrong — something a specific tenant or
//! DataSource is doing. [`lifecycle_events`] covers the registry and refresher
//! themselves, where the subject is the process rather than any one resource.

/// Anomalies attributable to a single named resource.
mod resource_events;

/// The registry and refresher's own lifecycle.
mod lifecycle_events;

pub(crate) use lifecycle_events::{
    primed, refresh_failed, refresher_started, refresher_stopped, snapshot_applied,
};
pub(crate) use resource_events::{
    divergent_payload_at_same_revision, duplicate_key_rejected, invalid_resource_rejected,
    stale_resource_ignored,
};
