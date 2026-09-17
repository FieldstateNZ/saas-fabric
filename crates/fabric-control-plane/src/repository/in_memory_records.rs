//! The two document shapes [`InMemoryClientRepository`](super::InMemoryClientRepository)
//! actually stores: rendered text and the revision that write produced,
//! never the typed struct — see the repository's own field docs for why.

use fabric_client_model::ClientRevision;

/// The catalogue as this repository actually keeps it: rendered text and the
/// revision that write produced.
pub(super) struct CatalogueRecord {
    /// The revision this text was written at.
    pub(super) revision: ClientRevision,

    /// The rendered document — what `Catalogue::render` produced, and what
    /// `Catalogue::parse` reads back.
    pub(super) text: String,
}

/// A client document as this repository actually keeps it: rendered text and
/// the revision that write produced — [`CatalogueRecord`]'s sibling for the
/// other kind of document this repository holds.
pub(super) struct ClientRecord {
    /// The revision this text was written at.
    pub(super) revision: ClientRevision,

    /// The rendered document — what `ClientDocument::render` produced, and
    /// what `ClientDocument::parse` reads back.
    pub(super) text: String,
}
