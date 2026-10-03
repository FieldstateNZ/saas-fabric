//! One registry, as the control plane asks things of it.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_control_plane::{Readability, RegistryClient, Repository};
use fabric_platform_management::RegistryError;
use fabric_registry::OciRegistry;

/// An [`OciRegistry`], behind the control plane's port.
///
/// The connector recognises its own when the set is handed back to install,
/// and installs the registry inside it: the client that proved a change is
/// the one discovery reads through, with the tokens and refusal it holds.
pub(super) struct Adapted(pub(super) Arc<OciRegistry>);

#[async_trait]
impl RegistryClient for Adapted {
    async fn prove(&self) -> Result<Option<String>, RegistryError> {
        let proof = self.0.prove().await?;
        Ok(proof.realm_origin().map(str::to_owned))
    }

    async fn prove_repository(&self, repository: &Repository) -> Result<Readability<()>, RegistryError> {
        Ok(answer(self.0.prove_repository(repository.as_str()).await?))
    }

    async fn version_tags(&self, repository: &Repository) -> Result<Readability<Vec<String>>, RegistryError> {
        Ok(answer(self.0.version_tags(repository.as_str()).await?))
    }

    fn credential_refused(&self) -> bool {
        self.0.credential_refused()
    }
}

/// The adapter's answer, in the control plane's words.
fn answer<T>(answer: fabric_registry::Readability<T>) -> Readability<T> {
    match answer {
        fabric_registry::Readability::Readable(found) => Readability::Readable(found),
        fabric_registry::Readability::NotReadable => Readability::NotReadable,
    }
}
