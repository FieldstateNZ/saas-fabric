//! The seam a runtime-state publisher writes through, and the only seam
//! anything may write through.

use async_trait::async_trait;

use crate::{
    PublicationCompleteness, PublicationError, PublicationReport, PublishedResources, PublishedRevisions,
    RuntimeSnapshot,
};

/// Publishes the runtime's three documents, and reports what is currently
/// held.
///
/// # What this port hides
///
/// Everything about *where* a document lands. There is no path here, no
/// `ConfigMap`, no directory, no rename. A caller offers a complete snapshot
/// at a revision it chose; whether that becomes three files on a local disk
/// (this crate's [`crate::FilesystemRuntimePublication`]) or three
/// `ConfigMap`s a Kubernetes controller patches (ADR 0018, "The Kubernetes
/// adapter" — not built here) is the implementation's business.
///
/// # Refusal is the port's job; the caller decides nothing about safety
///
/// [`publish`](Self::publish) takes the revision the caller believes it is
/// moving each document to, and an implementation **must** refuse a
/// publication that is stale, that diverges at an unchanged revision, that
/// would silently empty a document, or that breaks referential integrity
/// between the documents (ADR 0018 parts 4-6). A "last write wins" adapter
/// would satisfy this signature and quietly discard those guarantees; it
/// must not.
///
/// # Every call offers all three documents
///
/// There is no partial-publish method. See [`RuntimeSnapshot`]'s own
/// rustdoc for why independence between documents comes from their
/// separately advancing revisions rather than from being able to omit one.
#[async_trait]
pub trait RuntimePublication: Send + Sync {
    /// The revision currently held for each document, or `None` where no
    /// manifest has ever been published.
    ///
    /// # Errors
    ///
    /// Returns [`PublicationError`] if a manifest exists but could not be
    /// read or understood.
    async fn current(&self) -> Result<PublishedRevisions, PublicationError>;

    /// Every resource currently held, and the highest revision each has
    /// ever been published at, for a caller that composes its next offer
    /// against the last one: [`HeldDocuments::resources`](crate::HeldDocuments::resources).
    ///
    /// # Errors
    ///
    /// Returns [`PublicationError`] if what is held could not be read or
    /// understood, as [`publish`](Self::publish) would refuse it.
    async fn published_resources(&self) -> Result<PublishedResources, PublicationError>;

    /// Publishes a complete snapshot of all three documents.
    ///
    /// Refuses the *whole* snapshot, before a single byte is written, if any
    /// document's revision is stale, if any document diverges from what is
    /// held at an unchanged revision, if a tenant binding names a
    /// `DataSourceId` this same snapshot does not publish, if the data
    /// sources document drops an id the held tenants document still
    /// references, if a document would go from non-empty to empty without
    /// stating that intent, if the catalogue document is empty, if a tenant
    /// binding's `data` map is empty, if the tenants or data-sources
    /// document's held payload is lost while its manifest survives, if a
    /// resource does not advance past the revision it was last published
    /// at, if a held tenant is dropped, or if a catalogue entry names a
    /// logical data source no tenant binds.
    ///
    /// A publication that changes nothing writes nothing — not even a
    /// manifest whose revision did not move.
    ///
    /// # Errors
    ///
    /// Returns [`PublicationError`] naming exactly which rule the snapshot
    /// violated.
    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError>;

    /// Whether the target holds one whole recorded generation (G3): the
    /// held record, read before and again after every payload, manifest and
    /// the issuer registry it names, each checked against the record's
    /// revision and digest. Writes nothing and takes no publication lock.
    ///
    /// Never an error: a target that cannot be read, or whose record moved
    /// while it was read, is [`PublicationCompleteness::Unobserved`]. The
    /// answer carries no payload and no read error's detail.
    async fn inspect(&self) -> PublicationCompleteness;

    /// A short description for logging, such as the paths documents are
    /// published under.
    ///
    /// Must never contain a credential.
    fn describe(&self) -> String;
}
