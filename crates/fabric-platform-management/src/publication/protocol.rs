//! The offer-and-advance rule ADR 0023 part 4 (D3) names: publish at the
//! held revision, and advance only the one document the target reports
//! diverging -- never every document, and never by more than the target
//! actually asked for.

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;

use fabric_runtime_publication::{
    DocumentKind, DocumentRevision, PublicationError, PublicationReport, RuntimePublication, RuntimeSnapshot,
};

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

/// Offers `snapshot` to `target`, and on [`PublicationError::DivergentPayload`]
/// bumps exactly the named document's revision and offers the whole
/// snapshot again, up to [`MAX_RETRIES`] times.
///
/// Returns the [`PublicationReport`] and the snapshot actually accepted --
/// the caller needs the latter to know which revisions are now held,
/// without a second round trip to [`RuntimePublication::current`].
///
/// # Errors
///
/// Whatever [`RuntimePublication::publish`] refuses with, once retries are
/// exhausted or the refusal is not `DivergentPayload` -- a stale revision,
/// an emptying not intended, or any other rule the target enforces is
/// never retried, only reported.
pub(super) async fn publish_with_retry(
    target: &dyn RuntimePublication,
    mut snapshot: RuntimeSnapshot,
) -> Result<(PublicationReport, RuntimeSnapshot), PublicationError> {
    let mut retries = 0;

    loop {
        match target.publish(&snapshot).await {
            Ok(report) => return Ok((report, snapshot)),
            Err(PublicationError::DivergentPayload { document, revision }) if retries < MAX_RETRIES => {
                retries += 1;
                advance(&mut snapshot, document, revision);
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
