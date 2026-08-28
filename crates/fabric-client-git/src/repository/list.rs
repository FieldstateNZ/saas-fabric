//! Listing every client in the repository.

use fabric_client_model::ClientId;
use fabric_control_plane::{RepositoryError, StoredClient};

use crate::github::GitHost;
use crate::logging;
use crate::repository::GitClientRepository;

/// Reads every client under the configured directory.
///
/// # One request per client, and why that is accepted for now
///
/// The contents API returns a directory listing without file contents, so this
/// is one call for the listing plus one per client. At the scale SaaS Fabric
/// operates — tens of clients, one operator, a screen refreshed by hand — that
/// is a few hundred milliseconds and no cache to invalidate. It is written
/// down rather than left to be discovered, because the fix when it stops being
/// acceptable is a different API (a tree read, or a cached index), not a
/// tweak here.
///
/// # A document that will not parse fails the whole listing
///
/// The alternative — skip it, log it, show the rest — was considered and
/// rejected. A client silently missing from the operator console is the worst
/// possible presentation of a broken document: everything looks fine, and the
/// one client that needs attention is the one nobody can see. Failing names
/// the client and the rule it broke, which is what an operator can act on.
///
/// A directory with *no* document is skipped rather than failed, because that
/// is not a broken client — it is not a client at all.
pub(super) async fn list(
    repository: &GitClientRepository,
    host: &GitHost,
) -> Result<Vec<StoredClient>, RepositoryError> {
    let entries = host.list_directory().await?;
    let mut clients = Vec::new();

    for entry in entries {
        if !entry.is_directory {
            continue;
        }

        let Ok(client) = ClientId::try_new(&entry.name) else {
            logging::unnamed_directory_skipped();
            continue;
        };

        match repository.read(&client).await {
            Ok(stored) => clients.push(stored),
            Err(RepositoryError::NotFound { .. }) => logging::directory_without_document(&client),
            Err(error) => return Err(error),
        }
    }

    Ok(clients)
}
