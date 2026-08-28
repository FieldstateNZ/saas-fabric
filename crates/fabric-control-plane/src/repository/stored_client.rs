//! A client's document, and the revision it was read at.

use fabric_client_model::{ClientDocument, ClientRevision};

/// One client as the repository holds it right now.
///
/// The revision travels with the document rather than being fetched
/// separately, and that pairing is the whole optimistic-concurrency mechanism:
/// a caller that read this value knows exactly which version it is editing,
/// and hands that revision back when it writes. Splitting the two would make
/// it possible to read a document and write against a revision fetched a
/// moment later — which is precisely the race the mechanism exists to close.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredClient {
    /// The stored document.
    pub document: ClientDocument,

    /// The revision it was read at.
    pub revision: ClientRevision,
}
