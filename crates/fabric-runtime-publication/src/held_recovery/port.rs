//! The recovery port each publication adapter implements beside
//! [`crate::RuntimePublication`].

use async_trait::async_trait;

use super::{RestorePlan, RestoreRefusal};
use crate::{DocumentKind, PublicationError, TargetObservation};

/// Every held part, read raw, and whether the record stayed the same from
/// the first read to the last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldObservation {
    /// The parts.
    pub observation: TargetObservation,
    /// False when the record moved while the parts were read: they may come
    /// from two publications, and nothing is concluded from them.
    pub stable: bool,
}

/// Makes a restore's intent durable before anything is written.
pub trait RecoveryAudit: Send + Sync {
    /// Appends and syncs a record of the intent to carry out `plan`. Called
    /// once the restore is decided, under the adapter's exclusion, and
    /// before the first write. An error stops the restore with nothing
    /// written. Intent is not completion: the caller records the outcome.
    ///
    /// # Errors
    ///
    /// The I/O error that kept the intent from being made durable.
    fn intent(&self, plan: &RestorePlan) -> std::io::Result<()>;
}

/// Why a restore did not complete, and what that leaves behind.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    /// Refused before any intent was recorded. Nothing was written.
    #[error(transparent)]
    Refused(#[from] RestoreRefusal),

    /// Reading the target or taking its exclusion failed, before any intent
    /// was recorded. Nothing was written.
    #[error(transparent)]
    Target(#[from] PublicationError),

    /// The intent could not be made durable. Nothing was written.
    #[error("the restore's intent could not be audited, so nothing was written: {0}")]
    Audit(#[source] std::io::Error),

    /// A write failed before it could replace anything. Nothing was
    /// written; the intent is on record.
    #[error("the restore failed before it replaced anything: {0}")]
    Failed(#[source] PublicationError),

    /// A write failed after a file may have been replaced, or replaced but
    /// not made durable. The target may hold the restored bytes, part of
    /// them, or none: inspect again before anything else.
    #[error("the restore's outcome is uncertain; inspect the target again: {0}")]
    Uncertain(#[source] PublicationError),
}

/// Reads a publication target raw, and restores a blocking document in it.
#[async_trait]
pub trait HeldRecovery: Send + Sync {
    /// Every held part, raw. Takes no lock and writes nothing.
    ///
    /// # Errors
    ///
    /// [`PublicationError::Unreadable`] when a part cannot be read at all.
    async fn observe(&self) -> Result<HeldObservation, PublicationError>;

    /// Restores `document` from `offered` when [`super::plan_restore`]
    /// allows it, under an exclusion every cooperating publisher honours
    /// from before the read to the last write, calling `audit` before the
    /// first write. A plan with nothing to write is returned without an
    /// intent: the bytes are already restored.
    ///
    /// # Errors
    ///
    /// A [`RecoveryError`] saying what stopped it and what it left.
    async fn restore(
        &self,
        document: DocumentKind,
        offered: &[u8],
        audit: &dyn RecoveryAudit,
    ) -> Result<RestorePlan, RecoveryError>;

    /// Where the target is, for the report and the audit record.
    fn describe(&self) -> String;
}
