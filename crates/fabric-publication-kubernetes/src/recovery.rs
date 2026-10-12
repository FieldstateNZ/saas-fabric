//! [`HeldRecovery`] on Kubernetes: inspection only.
//!
//! A data-sources restore is authorised by the record on the tenants
//! object, a different object from the one it would write. A
//! `resourceVersion` condition on the written object, and any number of
//! reads of the other, cannot bind the write to that record: another
//! publisher can replace the record in between and the stale write still
//! lands. Only established, exclusive, fenced publisher ownership could, and
//! none exists (a `Lease` alone is not one). So every restore here is
//! refused before anything is read or audited, and the operator repairs
//! from inspection, as before.

use async_trait::async_trait;
use fabric_runtime_publication::{
    DocumentKind, HeldObservation, HeldRecovery, ObservedDocument, PublicationError, RecoveryAudit,
    RecoveryError, RestorePlan, RestoreRefusal, RuntimePublication, TargetObservation, ISSUER_REGISTRY_FILE,
    PUBLICATION_RECORD_FILE,
};

use crate::client::Found;
use crate::errors::unreadable;
use crate::object::{name_of, path_of, Keys};
use crate::wire::ConfigMap;
use crate::KubernetesRuntimePublication;

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
}

#[async_trait]
impl HeldRecovery for KubernetesRuntimePublication {
    /// Reads the tenants object, the other two, then the tenants object
    /// again: unstable when its `resourceVersion` moved in between.
    async fn observe(&self) -> Result<HeldObservation, PublicationError> {
        let tenants = self.fetch(DocumentKind::Tenants).await?;
        let data_sources = self.fetch(DocumentKind::DataSources).await?;
        let catalog = self.fetch(DocumentKind::Catalog).await?;
        let again = self.fetch(DocumentKind::Tenants).await?;
        let version = |object: &Option<ConfigMap>| {
            object
                .as_ref()
                .map(|object| object.metadata.resource_version.clone())
        };
        Ok(HeldObservation {
            stable: version(&tenants) == version(&again),
            observation: observation(tenants.as_ref(), data_sources.as_ref(), catalog.as_ref()),
        })
    }

    async fn restore(
        &self,
        _document: DocumentKind,
        _offered: &[u8],
        _audit: &dyn RecoveryAudit,
    ) -> Result<RestorePlan, RecoveryError> {
        Err(RestoreRefusal::OwnershipUnestablished.into())
    }

    fn describe(&self) -> String {
        RuntimePublication::describe(self)
    }
}

fn observation(
    tenants: Option<&ConfigMap>,
    data_sources: Option<&ConfigMap>,
    catalog: Option<&ConfigMap>,
) -> TargetObservation {
    let key = |object: Option<&ConfigMap>, key: &str| {
        object
            .and_then(|object| object.data.get(key))
            .map(|text| text.as_bytes().to_vec())
    };
    let document = |object: Option<&ConfigMap>, kind: DocumentKind| ObservedDocument {
        manifest: key(object, Keys::of(kind).manifest),
        payload: key(object, Keys::of(kind).payload),
    };
    TargetObservation {
        record: key(tenants, PUBLICATION_RECORD_FILE),
        tenants: document(tenants, DocumentKind::Tenants),
        data_sources: document(data_sources, DocumentKind::DataSources),
        catalog: document(catalog, DocumentKind::Catalog),
        registry: key(tenants, ISSUER_REGISTRY_FILE),
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
