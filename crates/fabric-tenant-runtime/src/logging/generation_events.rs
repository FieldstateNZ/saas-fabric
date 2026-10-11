//! Log events about reading the published documents as one generation.

use fabric_core::{event_id, EventType};

use crate::{SourceError, DOMAIN_ID};

/// A complete generation was applied that differs from the last one applied.
pub(crate) fn generation_applied(source: &str, generation: u64) {
    tracing::info!(
        event = "runtime.generation_applied",
        event_id = event_id(DOMAIN_ID, EventType::Success, 5),
        source,
        generation,
        "applied a complete publication generation"
    );
}

/// No generation could be applied, so both registries keep the last
/// complete one.
///
/// A warning when the documents are merely mid-publication -- the expected
/// state while a publication lands, or while one Kubernetes volume is
/// projected ahead of another -- and an error otherwise: a refused floor or
/// an unreadable record does not resolve itself.
pub(crate) fn generation_held(source: &str, error: &SourceError) {
    if matches!(error, SourceError::IncompleteGeneration { .. }) {
        tracing::warn!(
            event = "runtime.generation_incomplete",
            event_id = event_id(DOMAIN_ID, EventType::Warning, 5),
            source,
            reason = %error,
            "publication generation incomplete; continuing to serve the last complete generation"
        );
    } else {
        tracing::error!(
            event = "runtime.generation_refused",
            event_id = event_id(DOMAIN_ID, EventType::Error, 6),
            source,
            reason = %error,
            "publication generation refused; continuing to serve the last complete generation"
        );
    }
}

/// A read of the published documents outlasted the refresh interval and was
/// abandoned; both registries keep the last complete generation.
pub(crate) fn generation_timed_out(source: &str, timeout_seconds: u64) {
    tracing::error!(
        event = "runtime.generation_timed_out",
        event_id = event_id(DOMAIN_ID, EventType::Error, 7),
        source,
        timeout_seconds,
        "reading the published documents did not finish within the refresh interval; \
         continuing to serve the last complete generation"
    );
}
