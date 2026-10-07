//! The offer-and-advance rule ADR 0023 part 4 (D3) names: publish at the
//! held revision, and advance only the one document the target reports
//! diverging -- never every document, and never by more than the target
//! actually asked for. Also where an interrupted offer is re-offered at once
//! rather than left half-applied until the next pass.

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;

use fabric_runtime_publication::{
    DocumentKind, DocumentRevision, PublicationError, PublicationReport, RuntimePublication, RuntimeSnapshot,
};

use crate::SafeDiagnostic;

/// How many times a divergent document may be bumped and re-offered in one
/// pass, before the pass gives up and refuses.
///
/// A [`RuntimeSnapshot`] carries three documents, so three retries gives
/// every document its own chance to be re-offered once. A fourth
/// divergence past that means something worse than a stale byte comparison
/// is going on -- a concurrent writer this control plane does not know
/// about, or a document that keeps changing under it -- and chasing it
/// further would turn a pass that should finish in one HTTP round trip per
/// document into an unbounded loop.
const MAX_RETRIES: u8 = 3;

/// How many times a snapshot whose write was interrupted
/// ([`PublicationError::Unwritable`]) is re-offered at once, in the same pass.
///
/// `Unwritable` is the one refusal that may leave earlier documents written
/// and later ones not, so readers can serve a mixed set until the snapshot
/// is offered again (gap G3 in `docs/roadmap/m2-publication-gap-report.md`).
/// Waiting for the next scheduled pass keeps that window open for a whole
/// interval. Re-offering is safe: the plan re-reads what is held, documents
/// already written settle as unchanged, and the rest are written. Two, not
/// more, because a failure that survives three offers back to back is not a
/// blip -- a refused credential or a full disk -- and the next pass is the
/// right place to try again. No delay between offers: this crate takes no
/// timers from the runtime (see its `Cargo.toml`). `Unwritable` also covers
/// refusals that wrote nothing -- a filesystem publication lock held by
/// another publisher, or a Kubernetes object that moved after it was read --
/// and the same re-offer is right for those: it re-reads and re-plans.
const MAX_INTERRUPTED_RETRIES: u8 = 2;

/// Offers `snapshot` to `target`, and on [`PublicationError::DivergentPayload`]
/// bumps exactly the named document's revision and offers the whole
/// snapshot again, up to [`MAX_RETRIES`] times. On
/// [`PublicationError::Unwritable`] offers the same snapshot again at once,
/// up to [`MAX_INTERRUPTED_RETRIES`] times.
///
/// Returns the [`PublicationReport`] and the snapshot actually accepted --
/// the caller needs the latter to know which revisions are now held,
/// without a second round trip to [`RuntimePublication::current`].
///
/// # Errors
///
/// Whatever [`RuntimePublication::publish`] refuses with, once retries are
/// exhausted or the refusal is neither `DivergentPayload` nor `Unwritable`
/// -- a stale revision, an emptying not intended, or any other rule the
/// target enforces is never retried, only reported.
pub(super) async fn publish_with_retry(
    target: &dyn RuntimePublication,
    mut snapshot: RuntimeSnapshot,
) -> Result<(PublicationReport, RuntimeSnapshot), PublicationError> {
    let mut retries = 0;
    let mut interrupted = 0;

    loop {
        match target.publish(&snapshot).await {
            Ok(report) => return Ok((report, snapshot)),
            Err(PublicationError::DivergentPayload { document, revision }) if retries < MAX_RETRIES => {
                retries += 1;
                advance(&mut snapshot, document, revision);
            }
            Err(error @ PublicationError::Unwritable { .. }) if interrupted < MAX_INTERRUPTED_RETRIES => {
                interrupted += 1;
                tracing::warn!(
                    event = "control_plane.publication.reoffered",
                    target = target.describe(),
                    attempt = interrupted,
                    reason = %SafeDiagnostic::sanitise(&error.to_string()),
                    "a publication did not complete; offering it again now"
                );
            }
            Err(other) => return Err(other),
        }
    }
}

/// Bumps exactly the document the target named, to one past the revision it
/// was offered at -- not one past whatever `snapshot` currently carries, so
/// a document that diverges twice in one pass advances from what was
/// actually offered each time, never from a value this loop lost track of.
fn advance(snapshot: &mut RuntimeSnapshot, document: DocumentKind, offered: DocumentRevision) {
    let next = DocumentRevision::new(offered.get().saturating_add(1));

    match document {
        DocumentKind::Tenants => snapshot.tenants.revision = next,
        DocumentKind::DataSources => snapshot.data_sources.revision = next,
        DocumentKind::Catalog => snapshot.catalog.revision = next,
    }
}
