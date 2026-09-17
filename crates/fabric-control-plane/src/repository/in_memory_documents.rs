//! The render-then-parse round trip the in-memory store applies to a
//! client document, on the way in and on the way out.

use fabric_client_model::{ClientDocument, ClientId};

use crate::repository::in_memory::ClientRecord;
use crate::repository::{RepositoryError, StoredClient};

/// Renders `document`, then parses the result back with exactly the code
/// that will read it again — the same round trip the Git-backed and local
/// stores apply to every client document, so a bug in that round trip is
/// caught here too, rather than only in a store nothing but production runs.
pub(super) fn render(document: &ClientDocument) -> Result<String, RepositoryError> {
    let text = document.render().map_err(|source| RepositoryError::Invalid {
        client: document.client().id.clone(),
        source,
    })?;

    ClientDocument::parse(&text).map_err(|source| RepositoryError::Invalid {
        client: document.client().id.clone(),
        source,
    })?;

    Ok(text)
}

/// Parses a stored record back into a [`StoredClient`] — the same
/// discipline `catalogue` applies for the catalogue: an unreadable document
/// is `Invalid`, not `Unavailable`, because asking again would not fix a
/// document that does not parse.
pub(super) fn stored(client: &ClientId, record: &ClientRecord) -> Result<StoredClient, RepositoryError> {
    ClientDocument::parse(&record.text)
        .map(|document| StoredClient {
            document,
            revision: record.revision.clone(),
        })
        .map_err(|source| RepositoryError::Invalid {
            client: client.clone(),
            source,
        })
}
