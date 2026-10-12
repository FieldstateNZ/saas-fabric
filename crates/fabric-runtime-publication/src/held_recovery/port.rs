//! The recovery port each publication adapter implements beside
//! [`crate::RuntimePublication`].

use async_trait::async_trait;

use super::{RestorePlan, RestoreRefusal};
use crate::{DocumentKind, PublicationError, TargetObservation};

/// Records a restore before it happens.
pub trait RecoveryAudit: Send + Sync {
    /// Appends a durable record of `plan`. Called once the restore is
    /// decided and before anything is written; an error stops the restore
    /// with nothing written.
    ///
    /// # Errors
    ///
    /// The I/O error that kept the record from being made durable.
    fn before(&self, plan: &RestorePlan) -> std::io::Result<()>;

    /// Records that an audited restore then failed to write. Best effort:
    /// the write's own error is what the caller reports.
    fn failed(&self, plan: &RestorePlan, error: &PublicationError);
}

/// Why a restore did not complete.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    /// Refused before anything was audited or written.
    #[error(transparent)]
    Refused(#[from] RestoreRefusal),

    /// The audit record could not be made durable, so nothing was written.
    #[error("the restore could not be audited, so nothing was written: {0}")]
    Audit(#[source] std::io::Error),

    /// Reading, excluding other writers, or writing the target failed.
    /// `Unreadable`, and `Unwritable` from the lock or a moved object, mean
    /// nothing was written; `Unwritable` from a write after the audit may
    /// leave the payload restored and the manifest not, which a second
    /// restore completes.
    #[error(transparent)]
    Target(#[from] PublicationError),
}

/// Reads a publication target raw, and restores a blocking document in it.
#[async_trait]
pub trait HeldRecovery: Send + Sync {
    /// Every held part, raw. Writes nothing.
    ///
    /// # Errors
    ///
    /// [`PublicationError::Unreadable`] when a part cannot be read at all.
    async fn observe(&self) -> Result<TargetObservation, PublicationError>;

    /// Restores `document` from `offered` when [`super::plan_restore`]
    /// allows it, excluding every other writer that goes through the
    /// adapter from the read to the last write, and calling `audit` before
    /// the first write.
    ///
    /// # Errors
    ///
    /// A [`RecoveryError`] saying what stopped it.
    async fn restore(
        &self,
        document: DocumentKind,
        offered: &[u8],
        audit: &dyn RecoveryAudit,
    ) -> Result<RestorePlan, RecoveryError>;

    /// Where the target is, for the report and the audit record.
    fn describe(&self) -> String;
}
