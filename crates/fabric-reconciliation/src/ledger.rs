//! What Fabric itself last wrote for each declared client, kept durably
//! outside the provider and outside desired state (D01-15, D01-19b).
//!
//! # Why reconciliation needs it
//!
//! A declared callback the provider lacks is one of two things: a callback
//! Fabric wrote and somebody removed by hand, which is held, or one Fabric has
//! not written yet — a host the operator just added — which is product-owned
//! and written. The provider cannot tell them apart and neither can the
//! declaration, so without a record of what was written a legitimate host
//! change was held as a removal while the old callbacks were dropped, and a
//! move between host kinds left a client with none.
//!
//! # What may answer
//!
//! Only a durable record Fabric appended itself, bound to the client, its
//! realm and the digest of each declared callback: not desired-state bytes,
//! which anyone with write access edits; not provider metadata, which a hand
//! edit can change; and not a process cache, which a restart empties. A
//! deployment with no ledger knows no baseline, and every ambiguous
//! transition is held until an operator re-asserts.
//!
//! # Two stages, so a failure is never a false baseline
//!
//! A write is recorded as [`WriteStage::Writing`] **before** the provider is
//! called, and as [`WriteStage::Written`] once it confirms. A callback that is
//! only in a `Writing` record may or may not have reached the provider, so a
//! missing one is ambiguous and held — never re-written over a possible hand
//! removal, never reported as one. A provider write is not made at all when
//! its `Writing` record could not be appended.

mod baseline;
mod digest;

use async_trait::async_trait;
use fabric_client_model::{Client, OidcClient};

pub use baseline::{callback_digests, AppliedBaseline, WrittenCallbacks};
pub use digest::CallbackDigest;

/// Which side of a provider write a ledger record is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStage {
    /// Appended before the provider is asked to write the declaration.
    Writing,

    /// Appended once the provider confirmed the write.
    Written,
}

/// Why the ledger could not answer or record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// The record could not be read, or did not verify.
    #[error("the declaration ledger could not be read: {detail}")]
    Unreadable {
        /// What the adapter observed. No callback, no credential.
        detail: String,
    },

    /// A record could not be appended.
    #[error("the declaration ledger could not record the write: {detail}")]
    Unrecorded {
        /// What the adapter observed. No callback, no credential.
        detail: String,
    },
}

/// The durable record of what Fabric wrote for each declared client.
///
/// Implemented by the composition root over a store it trusts; see this
/// module's rustdoc for what qualifies.
#[async_trait]
pub trait DeclarationLedger: Send + Sync {
    /// What has been recorded for `client`'s declared clients in its current
    /// realm. A declared client with no record is absent: unknown, never an
    /// empty baseline.
    ///
    /// # Errors
    ///
    /// [`LedgerError::Unreadable`] when the record cannot be read or does
    /// not verify. The caller treats every client as unknown.
    async fn read(&self, client: &Client) -> Result<AppliedBaseline, LedgerError>;

    /// Appends that `declared` is being, or has been, written for `client`.
    ///
    /// # Errors
    ///
    /// [`LedgerError::Unrecorded`] when the record was not stored.
    async fn record(
        &self,
        client: &Client,
        declared: &OidcClient,
        stage: WriteStage,
    ) -> Result<(), LedgerError>;
}
