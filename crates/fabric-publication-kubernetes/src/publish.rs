//! The port implementation: read three objects, plan, write what changed.

use async_trait::async_trait;
use fabric_runtime_publication::{
    plan_publication, DocumentKind, DocumentOutcome, DocumentPlan, PublicationError, PublicationReport,
    PublishedRevisions, RuntimePublication, RuntimeSnapshot,
};

use crate::client::Client;
use crate::held::{Read, Reads};
use crate::object::{collection_of, object_for, path_of};
use crate::PublicationTarget;

/// Publishes the runtime's three documents as `ConfigMap`s in one namespace,
/// with the pod's own identity.
pub struct KubernetesRuntimePublication {
    pub(crate) client: Client,
    namespace: String,
}

impl KubernetesRuntimePublication {
    /// The in-cluster adapter.
    ///
    /// # Errors
    ///
    /// A message when the namespace is not a DNS label or the cluster trust
    /// root cannot be read. Never a path or a credential.
    pub fn in_cluster(target: PublicationTarget) -> Result<Self, String> {
        target.validate()?;
        Ok(Self {
            client: Client::in_cluster()?,
            namespace: target.namespace,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_client(client: Client, namespace: &str) -> Self {
        Self {
            client,
            namespace: namespace.to_owned(),
        }
    }

    async fn write(
        &self,
        document: DocumentKind,
        plan: &DocumentPlan,
        read: &Read,
    ) -> Result<(), PublicationError> {
        if plan.outcome == DocumentOutcome::Unchanged {
            return Ok(());
        }
        let object = object_for(&self.namespace, document, plan, read.resource_version.clone())?;
        let path = if read.resource_version.is_some() {
            path_of(&self.namespace, document)
        } else {
            collection_of(&self.namespace)
        };
        self.client.write(&path, &object, document).await
    }
}

#[async_trait]
impl RuntimePublication for KubernetesRuntimePublication {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        Ok(Reads::fetch(&self.client, &self.namespace)
            .await?
            .held()
            .revisions())
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        let reads = Reads::fetch(&self.client, &self.namespace).await?;
        let plan = plan_publication(snapshot, &reads.held())?;

        // Every object is built — and so size-checked — before the first is
        // written, so a document too large to publish refuses the whole
        // publication rather than leaving the cluster half-updated.
        object_for(
            &self.namespace,
            DocumentKind::DataSources,
            &plan.data_sources,
            None,
        )?;
        object_for(&self.namespace, DocumentKind::Catalog, &plan.catalog, None)?;
        object_for(&self.namespace, DocumentKind::Tenants, &plan.tenants, None)?;

        // ADR 0018 part 3's order: a data source before anything naming it,
        // and tenants last.
        self.write(DocumentKind::DataSources, &plan.data_sources, &reads.data_sources)
            .await?;
        self.write(DocumentKind::Catalog, &plan.catalog, &reads.catalog)
            .await?;
        self.write(DocumentKind::Tenants, &plan.tenants, &reads.tenants)
            .await?;

        Ok(PublicationReport {
            tenants: plan.tenants.outcome,
            data_sources: plan.data_sources.outcome,
            catalog: plan.catalog.outcome,
        })
    }

    fn describe(&self) -> String {
        format!("kubernetes: three ConfigMaps in namespace {}", self.namespace)
    }
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
