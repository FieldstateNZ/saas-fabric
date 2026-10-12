//! [`HeldRecovery`] on the filesystem: the restore runs under the same
//! publication lock as `publish`, and writes through the same `atomic_write`.

use async_trait::async_trait;

use super::adapter::FilesystemRuntimePublication;
use super::atomic_write::{atomic_write, remove_stale_staging};
use super::inspect::observe;
use super::lock::PublicationLock;
use super::paths::DocumentPaths;
use crate::{
    plan_restore, DocumentKind, HeldRecovery, PublicationError, RecoveryAudit, RecoveryError, RestorePlan,
    RestoreRefusal, RuntimePublication, TargetObservation,
};

impl FilesystemRuntimePublication {
    fn observe_raw(&self) -> Result<TargetObservation, PublicationError> {
        observe(
            [&self.tenants, &self.data_sources, &self.catalog],
            &self.record,
            &self.registry,
        )
        .map_err(|cause| PublicationError::Unreadable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        })
    }
}

#[async_trait]
impl HeldRecovery for FilesystemRuntimePublication {
    async fn observe(&self) -> Result<TargetObservation, PublicationError> {
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
        // Held from the read to the last write, as `publish` holds it: no
        // publication through this adapter can plan against, or write over,
        // a document half restored.
        let _lock = PublicationLock::acquire(&self.tenants)?;
        remove_stale_staging(&paths.payload);
        remove_stale_staging(&paths.manifest);
        let plan = plan_restore(&self.observe_raw()?, document, offered)?;
        audit.before(&plan).map_err(RecoveryError::Audit)?;
        if let Err(error) = write(paths, &plan) {
            audit.failed(&plan, &error);
            return Err(error.into());
        }
        Ok(plan)
    }

    fn describe(&self) -> String {
        RuntimePublication::describe(self)
    }
}

/// The payload, then the manifest, as `write_if_needed` writes a document.
fn write(paths: &DocumentPaths, plan: &RestorePlan) -> Result<(), PublicationError> {
    let unwritable = |cause: Box<dyn std::error::Error + Send + Sync>| PublicationError::Unwritable {
        document: paths.kind,
        cause,
    };
    if let Some(payload) = &plan.payload {
        atomic_write(&paths.payload, payload).map_err(|cause| unwritable(Box::new(cause)))?;
    }
    if let Some(manifest) = &plan.manifest {
        let bytes = manifest
            .canonical_json()
            .map_err(|cause| unwritable(Box::new(cause)))?;
        atomic_write(&paths.manifest, &bytes).map_err(|cause| unwritable(Box::new(cause)))?;
    }
    Ok(())
}
