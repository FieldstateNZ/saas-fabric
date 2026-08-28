//! Writing one client's document.

use fabric_client_model::{ClientDocument, ClientId, ClientRevision};
use fabric_control_plane::{ChangeContext, RepositoryError};

use crate::github::GitHost;
use crate::logging;

/// Renders the document, checks it reads back, and commits it conditionally.
///
/// # The round trip is the "nothing malformed is committed" guarantee
///
/// The document handed in is valid by construction — `ClientDocument` has no
/// constructor that produces an invalid one. That is an argument, though, and
/// this is a commit to the platform's source of truth, so the argument is
/// checked: the rendered text is parsed with exactly the code that will read
/// it back, and a failure aborts *before* the write rather than leaving a
/// document in Git that the control plane can no longer read.
///
/// The cost is one parse per edit, on a path already making a network call.
pub(super) async fn update(
    host: &GitHost,
    client: &ClientId,
    document: &ClientDocument,
    expected: &ClientRevision,
    change: &ChangeContext,
) -> Result<ClientRevision, RepositoryError> {
    let text = document.render().map_err(|source| RepositoryError::Invalid {
        client: client.clone(),
        source,
    })?;

    ClientDocument::parse(&text).map_err(|source| RepositoryError::Invalid {
        client: client.clone(),
        source,
    })?;

    let revision = host
        .write_document(client, &text, expected, &commit_message(change))
        .await?;

    logging::client_written(client, &revision);

    Ok(revision)
}

/// The commit message a change produces.
///
/// # Why the operator is named in a trailer
///
/// Every commit is authored by the platform's machine identity, because that
/// is who holds the token. Without a trailer, Git's own history would record
/// only that SaaS Fabric changed a client and never who asked — so the trailer
/// carries the requester, in the same shape Git tooling already understands
/// (§24).
///
/// It is a *second* copy of the audit record, not the only one. The control
/// plane emits its own event, because a refused write leaves no commit and is
/// still worth knowing about.
fn commit_message(change: &ChangeContext) -> String {
    format!("{}\n\nRequested-by: {}\n", change.summary, change.requested_by)
}
