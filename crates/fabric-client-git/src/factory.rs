//! Building a desired-state repository from an established integration.
//!
//! # Why the credential no longer comes from configuration
//!
//! It used to: a deployment named an application id, an installation id and a
//! secret reference, and the composition root assembled a credential from
//! them once at startup. All three are now things the platform learned when an
//! operator connected it, so the credential is assembled here instead — every
//! time the integration changes, from state the platform owns.

use std::sync::Arc;

use fabric_control_plane::{ClientRepository, DesiredStateFactory, GitIntegration, SecretValue};
use fabric_core::Clock;

use crate::{GitAuthConfig, GitClientRepository, GitCredential, GitRepositoryConfig};

/// Builds GitHub-backed repositories for whatever is currently connected.
pub struct GitRepositoryFactory {
    /// Where the API lives.
    api_base_url: String,

    /// How the platform identifies itself in a commit.
    committer_name: String,

    /// The address commits are attributed to.
    committer_email: String,

    /// How long a request may take.
    http_timeout_seconds: u64,

    /// Stamps token lifetimes.
    clock: Arc<dyn Clock>,
}

impl GitRepositoryFactory {
    /// Builds a factory.
    #[must_use]
    pub fn new(
        api_base_url: impl Into<String>,
        committer_name: impl Into<String>,
        committer_email: impl Into<String>,
        http_timeout_seconds: u64,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            api_base_url: api_base_url.into(),
            committer_name: committer_name.into(),
            committer_email: committer_email.into(),
            http_timeout_seconds,
            clock,
        }
    }
}

impl DesiredStateFactory for GitRepositoryFactory {
    fn build(
        &self,
        integration: &GitIntegration,
        private_key: &SecretValue,
    ) -> Result<Arc<dyn ClientRepository>, String> {
        let repository = integration
            .repository()
            .ok_or_else(|| "the integration has not settled on a repository".to_owned())?;

        let installation = integration
            .installation
            .as_ref()
            .ok_or_else(|| "the application has not been installed".to_owned())?;

        let config = GitRepositoryConfig {
            api_base_url: self.api_base_url.clone(),
            owner: repository.owner.clone(),
            repository: repository.name.clone(),
            branch: repository.branch.clone(),
            path_prefix: repository.path_prefix.clone(),
            document_file: repository.document_file.clone(),
            committer_name: self.committer_name.clone(),
            committer_email: self.committer_email.clone(),
            http_timeout_seconds: self.http_timeout_seconds,

            // The identifiers are carried so that the configuration this
            // adapter already validates stays complete and self-describing.
            // The credential below is what is actually presented; the
            // reference here names nothing that will be resolved, because the
            // key has already been resolved by the caller.
            auth: GitAuthConfig::GithubApp {
                app_id: integration.app_id.clone(),
                installation_id: installation.id.clone(),
                private_key_ref: "git/app-private-key".to_owned(),
            },
        };

        let credential = GitCredential::app(&integration.app_id, &installation.id, private_key.expose());

        Ok(Arc::new(GitClientRepository::new(
            &config,
            credential,
            Arc::clone(&self.clock),
        )?))
    }
}
