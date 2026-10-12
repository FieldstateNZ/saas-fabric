//! The port every audit store implements.

use async_trait::async_trait;

use crate::audit_trail::{verify, AuditEntry, AuditError, AuditRecord, AuditStream, ChainBreak, ChainHead};

/// An append-only, hash-chained store of audit entries (ADR 0028).
///
/// A stateless I/O seam, so it is boxed: the host chooses the adapter, and
/// tests substitute the in-memory one.
///
/// # What every implementation must hold to
///
/// - **Append-only.** There is no update, delete or truncate here, and an
///   implementation must not offer one by another route.
/// - **One chain per stream.** An append seals `record` after its stream's
///   current head, under a lock that covers reading the head and writing the
///   entry, so two concurrent appends can never both claim one sequence.
/// - **Refuse, never shorten.** An entry over the size limit is refused with
///   [`AuditError::EntryTooLarge`].
/// - **Refuse to extend a broken stream.** A stream that does not verify
///   answers [`AuditError::Broken`] rather than accepting more entries.
/// - **Nothing prunes.** No retention is implemented; see ADR 0028.
#[async_trait]
pub trait AuditTrail: Send + Sync {
    /// Seals `record` into its stream, stamped `at` (Unix seconds), and
    /// stores it durably before returning.
    ///
    /// # Errors
    ///
    /// As this trait's rustdoc lists, and [`AuditError::Unavailable`] if the
    /// store could not be written.
    async fn append(&self, record: AuditRecord, at: u64) -> Result<AuditEntry, AuditError>;

    /// Every stored entry of `stream`, in stored order, with the head the
    /// store recorded for it. Not verified: call [`StoredStream::verify`].
    ///
    /// # Errors
    ///
    /// [`AuditError::Malformed`] for a line that is not an entry, and
    /// [`AuditError::Unavailable`] if the store could not be read.
    async fn read(&self, stream: &AuditStream) -> Result<StoredStream, AuditError>;

    /// A one-line description for the startup log.
    fn describe(&self) -> String;
}

/// A stream as read back from a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredStream {
    /// Which stream.
    pub stream: AuditStream,
    /// Its entries, in stored order.
    pub entries: Vec<AuditEntry>,
    /// The head the store recorded at its last append, if any.
    pub anchor: Option<ChainHead>,
}

impl StoredStream {
    /// Verifies the stream against its recorded head.
    ///
    /// # Errors
    ///
    /// The first [`ChainBreak`] found.
    pub fn verify(&self) -> Result<Option<ChainHead>, ChainBreak> {
        verify(&self.stream, &self.entries, self.anchor.as_ref())
    }
}
