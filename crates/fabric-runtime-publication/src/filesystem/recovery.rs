//! [`HeldRecovery`] on the filesystem: the restore takes the same
//! publication lock as `publish` before it reads anything, holds it through
//! the plan, the audited intent and every write, and writes through the same
//! `atomic_write`.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! restore's order of steps, and the rule its writes' outcomes are
//! classified by, read together.

use std::path::Path;

use async_trait::async_trait;

use super::adapter::FilesystemRuntimePublication;
use super::atomic_write::{atomic_write, remove_stale_staging};
use super::inspect::{observe, read};
use super::lock::PublicationLock;
use super::paths::DocumentPaths;
use crate::{
    plan_restore, DocumentKind, HeldObservation, HeldRecovery, PublicationError, RecoveryAudit,
    RecoveryError, RestorePlan, RestoreRefusal, RuntimePublication,
};

impl FilesystemRuntimePublication {
    /// Reads every part, then the record again, so a record replaced while
    /// the parts were read is reported unstable rather than mixed in.
    fn observe_raw(&self) -> Result<HeldObservation, PublicationError> {
        let unreadable = |cause: std::io::Error| PublicationError::Unreadable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        };
        let observation = observe(
            [&self.tenants, &self.data_sources, &self.catalog],
            &self.record,
            &self.registry,
        )
        .map_err(unreadable)?;
        let after = read(&self.record).map_err(unreadable)?;
        Ok(HeldObservation {
            stable: after == observation.record,
            observation,
        })
    }
}

#[async_trait]
impl HeldRecovery for FilesystemRuntimePublication {
    async fn observe(&self) -> Result<HeldObservation, PublicationError> {
        self.observe_raw()
    }

    async fn restore(
        &self,
        document: DocumentKind,
        offered: &[u8],
        audit: &dyn RecoveryAudit,
    ) -> Result<RestorePlan, RecoveryError> {
        let paths = match document {
            DocumentKind::Tenants => &self.tenants,
            DocumentKind::DataSources => &self.data_sources,
            DocumentKind::Catalog => return Err(RestoreRefusal::NotRestorable { document }.into()),
        };
        // Taken before anything is read, and only where a publication made
        // its file: a refusal leaves the target exactly as it was.
        let _lock = PublicationLock::acquire_existing(&self.tenants)?;
        let plan = plan_restore(&self.observe_raw()?, document, offered)?;
        if plan.already_restored() {
            return Ok(plan);
        }
        audit.intent(&plan).map_err(RecoveryError::Audit)?;
        // Only now, with the intent durable and the lock held, is a staging
        // file a crashed writer left ours to clear.
        remove_stale_staging(&paths.payload);
        remove_stale_staging(&paths.manifest);
        write(paths, &plan)?;
        Ok(plan)
    }

    fn describe(&self) -> String {
        RuntimePublication::describe(self)
    }
}

/// The payload, then the manifest, as `write_if_needed` writes a document.
///
/// A failed write is checked against the file it targeted: if the file now
/// holds the new bytes (the rename happened, a later sync failed), or it
/// cannot be read, or an earlier write already landed, the outcome is
/// uncertain. Only a failure that provably replaced nothing is `Failed`.
fn write(paths: &DocumentPaths, plan: &RestorePlan) -> Result<(), RecoveryError> {
    let unwritable = |cause: Box<dyn std::error::Error + Send + Sync>| PublicationError::Unwritable {
        document: paths.kind,
        cause,
    };
    let manifest = plan
        .manifest
        .as_ref()
        .map(crate::DocumentManifest::canonical_json)
        .transpose()
        .map_err(|cause| RecoveryError::Failed(unwritable(Box::new(cause))))?;
    let steps = [
        (paths.payload.as_path(), plan.payload.as_deref()),
        (paths.manifest.as_path(), manifest.as_deref()),
    ];
    let mut landed = false;
    for (target, bytes) in steps {
        let Some(bytes) = bytes else { continue };
        if let Err(cause) = atomic_write(target, bytes) {
            let error = unwritable(Box::new(cause));
            return Err(if landed || holds(target, bytes) {
                RecoveryError::Uncertain(error)
            } else {
                RecoveryError::Failed(error)
            });
        }
        landed = true;
    }
    Ok(())
}

/// Whether `target` holds `bytes`: an absent file does not, and one that
/// cannot be read is counted as possibly holding them.
fn holds(target: &Path, bytes: &[u8]) -> bool {
    match std::fs::read(target) {
        Ok(held) => held == bytes,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}
