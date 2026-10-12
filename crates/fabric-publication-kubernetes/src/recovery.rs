//! [`HeldRecovery`] on Kubernetes: a restore replaces one object at the
//! `resourceVersion` it was read at, so the API server refuses it over any
//! other write, and keeps every key the object carries beside the document.

use async_trait::async_trait;
use fabric_runtime_publication::{
    plan_restore, DocumentKind, HeldRecovery, PublicationError, RecoveryAudit, RecoveryError, RestorePlan,
    RestoreRefusal, RuntimePublication, TargetObservation,
};

use crate::client::Found;
use crate::errors::{unreadable, unwritable};
use crate::object::{name_of, path_of};
use crate::wire::ConfigMap;
use crate::KubernetesRuntimePublication;

mod held_object;

use held_object::{observation, restored, version, Held};

impl KubernetesRuntimePublication {
    async fn fetch(&self, document: DocumentKind) -> Result<Option<ConfigMap>, PublicationError> {
        match self
            .client
            .get(&path_of(&self.namespace, document), document)
            .await?
        {
            Found::Absent => Ok(None),
            Found::Object(object) if object.metadata.name == name_of(document) => Ok(Some(object)),
            Found::Object(_) => Err(unreadable(
                document,
                "the cluster answered with a different object",
            )),
        }
    }

    async fn fetch_all(&self) -> Result<Held, PublicationError> {
        Ok([
            self.fetch(DocumentKind::Tenants).await?,
            self.fetch(DocumentKind::DataSources).await?,
            self.fetch(DocumentKind::Catalog).await?,
        ])
    }
}

#[async_trait]
impl HeldRecovery for KubernetesRuntimePublication {
    async fn observe(&self) -> Result<TargetObservation, PublicationError> {
        Ok(observation(&self.fetch_all().await?))
    }

    async fn restore(
        &self,
        document: DocumentKind,
        offered: &[u8],
        audit: &dyn RecoveryAudit,
    ) -> Result<RestorePlan, RecoveryError> {
        let held = self.fetch_all().await?;
        let plan = plan_restore(&observation(&held), document, offered)?;
        let [tenants, data_sources, _] = &held;
        let object = match document {
            DocumentKind::Tenants => tenants.as_ref(),
            DocumentKind::DataSources => data_sources.as_ref(),
            DocumentKind::Catalog => return Err(RestoreRefusal::NotRestorable { document }.into()),
        }
        .ok_or_else(|| unwritable(document, "the object to restore is not held"))?;
        let object = restored(object, &plan)?;
        // The record that vouches for a data-sources restore is on the
        // tenants object, which this write does not carry. Re-reading it
        // narrows, as `confirm.rs` does for a publication, the window in
        // which another writer could replace the record under this restore.
        if document == DocumentKind::DataSources {
            let now = self.fetch(DocumentKind::Tenants).await?;
            if version(now.as_ref()) != version(tenants.as_ref()) {
                return Err(unwritable(
                    DocumentKind::Tenants,
                    "another writer changed the publication record this restore relied on; nothing was written",
                )
                .into());
            }
        }
        audit.before(&plan).map_err(RecoveryError::Audit)?;
        let path = path_of(&self.namespace, document);
        if let Err(error) = self.client.write(&path, &object, document).await {
            audit.failed(&plan, &error);
            return Err(error.into());
        }
        Ok(plan)
    }

    fn describe(&self) -> String {
        RuntimePublication::describe(self)
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
