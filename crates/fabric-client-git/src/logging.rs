//! Structured log events for the Git-backed repository.
//!
//! Nothing here is handed a token or an upstream response body. The repository
//! description these lines carry is the owner, repository and branch — which
//! is operator telemetry, and never reaches an API response (§8).

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// A client's document was committed.
pub(crate) fn client_written(client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "clients_repository.client_written",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        client_id = %client,
        revision = %revision,
        "wrote a client's desired state"
    );
}

/// A directory under the clients path is not named like a client.
///
/// The name is deliberately not logged: it is content from a repository humans
/// also edit, and this is the one place such text could reach a log line.
pub(crate) fn unnamed_directory_skipped() {
    tracing::warn!(
        event = "clients_repository.unnamed_directory_skipped",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        "a directory under the clients path is not named like a client; ignoring it"
    );
}

/// A client directory holds no desired-state document.
///
/// Warn rather than error: it is almost always a half-created client, which is
/// a real thing to notice and not a reason to fail the listing.
pub(crate) fn directory_without_document(client: &ClientId) {
    tracing::warn!(
        event = "clients_repository.directory_without_document",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        client_id = %client,
        "a client directory holds no desired-state document; ignoring it"
    );
}
