//! The port this client implements.

use fabric_platform_management::{Attached, Registry, RegistryError, Resolved};

use crate::client::OciRegistry;

#[async_trait::async_trait]
impl Registry for OciRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.list_tags(repository).await
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        self.resolve_reference(repository, reference).await
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.attached(repository, subject).await
    }
}
