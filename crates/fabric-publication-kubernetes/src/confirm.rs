//! Refusing a publication whose plan relied on an object that has since
//! moved.

use fabric_runtime_publication::{DocumentKind, DocumentOutcome, PublicationError, PublicationPlan};

use crate::client::Client;
use crate::errors::unwritable;
use crate::held::{read_one, Read, Reads};

/// Re-reads every object the plan relied on but will not write, when it
/// will write anything at all, and refuses if any has moved.
///
/// A publication sends `resourceVersion` on every object it writes, so the
/// API server refuses a write over a version it did not see. An object the
/// plan only *read* -- the held tenants document behind the retirement
/// guard, the held data sources behind a new binding -- gets no such check,
/// and two writers each valid against what they read can together publish a
/// binding to a DataSource that is gone (gap G4, write skew). Reading those
/// objects again just before the first write narrows that window to the
/// moments between this check and the writes. It cannot close it: the
/// Kubernetes API has no transaction across objects.
///
/// # Errors
///
/// `Unwritable` for the object that moved, though nothing has been written:
/// it is the variant the controller re-offers at once, and the re-offer
/// re-reads and re-plans. `Unreadable` if an object cannot be re-read.
pub(crate) async fn confirm_read_only_unmoved(
    client: &Client,
    namespace: &str,
    plan: &PublicationPlan,
    reads: &Reads,
) -> Result<(), PublicationError> {
    let documents = [
        (
            DocumentKind::DataSources,
            plan.data_sources.outcome,
            &reads.data_sources,
        ),
        (DocumentKind::Catalog, plan.catalog.outcome, &reads.catalog),
        (DocumentKind::Tenants, plan.tenants.outcome, &reads.tenants),
    ];
    if documents
        .iter()
        .all(|(_, outcome, _)| *outcome == DocumentOutcome::Unchanged)
    {
        return Ok(());
    }
    for (document, outcome, read) in documents {
        if outcome == DocumentOutcome::Unchanged {
            confirm_unmoved(client, namespace, document, read).await?;
        }
    }
    Ok(())
}

async fn confirm_unmoved(
    client: &Client,
    namespace: &str,
    document: DocumentKind,
    read: &Read,
) -> Result<(), PublicationError> {
    let now = read_one(client, namespace, document).await?;
    if now.resource_version == read.resource_version {
        Ok(())
    } else {
        Err(unwritable(
            document,
            "another writer changed a published document this publication relied on; the next pass re-reads it",
        ))
    }
}
