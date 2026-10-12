//! The port implementation: read three objects, plan, write what changed.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! port's methods. What the tenants object carries, which the size check and
//! the writes must share, is `tenants_object.rs`.

use async_trait::async_trait;
use fabric_runtime_publication::{
    plan_publication, DocumentKind, DocumentManifest, DocumentOutcome, DocumentPlan, PublicationCompleteness,
    PublicationError, PublicationReport, PublishedResources, PublishedRevisions, RegistryOutcome,
    RuntimePublication, RuntimeSnapshot,
};

use crate::client::Client;
use crate::confirm::confirm_read_only_unmoved;
use crate::held::{Read, Reads};
use crate::object::{collection_of, object_for, path_of};
use crate::tenants_object::{objects, tenants_object_plan};
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
        manifest: &DocumentManifest,
        carried: &[(&'static str, &[u8])],
        read: &Read,
    ) -> Result<(), PublicationError> {
        if plan.outcome == DocumentOutcome::Unchanged {
            return Ok(());
        }
        let object = object_for(
            &self.namespace,
            document,
            plan,
            manifest,
            carried,
            read.resource_version.clone(),
        )?;
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

    async fn published_resources(&self) -> Result<PublishedResources, PublicationError> {
        Reads::fetch(&self.client, &self.namespace)
            .await?
            .held()
            .resources()
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        let reads = Reads::fetch(&self.client, &self.namespace).await?;
        let plan = plan_publication(snapshot, &reads.held())?;
        let tenants = tenants_object_plan(&plan);
        let objects = objects(&plan, &tenants, &reads);

        // Every object is built, so size-checked, before the first is written:
        // one too large refuses the whole publication, not half of it.
        for (document, plan, manifest, carried, _) in &objects {
            object_for(&self.namespace, *document, plan, manifest, carried, None)?;
        }

        confirm_read_only_unmoved(&self.client, &self.namespace, &plan, &reads).await?;

        // ADR 0018 part 3's order; tenants last, carrying the record and
        // the registry, so all three land in one object write.
        for (document, plan, manifest, carried, read) in &objects {
            self.write(*document, plan, manifest, carried, read).await?;
        }

        Ok(PublicationReport {
            tenants: plan.tenants.outcome,
            data_sources: plan.data_sources.outcome,
            catalog: plan.catalog.outcome,
            registry: plan.registry.as_ref().map(|registry| RegistryOutcome {
                outcome: registry.outcome,
                generation: registry.document.generation(),
            }),
            record: plan.record.stamp,
        })
    }

    async fn inspect(&self) -> PublicationCompleteness {
        crate::inspect::inspect(&self.client, &self.namespace).await
    }

    fn describe(&self) -> String {
        format!("kubernetes: three ConfigMaps in namespace {}", self.namespace)
    }
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
