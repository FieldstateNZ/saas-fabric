//! What Platform Management binds when its integration becomes usable.

use std::sync::Arc;

use fabric_control_plane::{GitIntegration, IntegrationTarget, SecretValue};
use fabric_core::Clock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{PlatformGitRepository, PlatformRepositoryConfig};
use fabric_platform_management::{DesiredState, PlatformDesiredState};

/// Composes the platform repository from a connected integration.
///
/// # Why this lives in the composition root
///
/// Its job is "when this integration becomes connected, build these runtime
/// services against the repository the operator chose" — which is composition,
/// not Git persistence. `fabric-platform-git` knows how to read and write a
/// platform repository and nothing about integrations, installations or
/// control-plane records, and it stays that way.
///
/// The client integration's factory sits inside its adapter crate instead, and
/// that is precedent rather than a pattern worth spreading: it gives that
/// adapter an edge to the control plane it does not otherwise need. Not
/// copying it here is one coupling nobody has to unwind later.
pub struct PlatformManagementTarget {
    /// Where the Git host's API lives.
    api_base_url: String,

    /// How long a call to it may take.
    http_timeout_seconds: u64,

    /// Stamps installation tokens.
    clock: Arc<dyn Clock>,

    /// What the platform reads and writes through once this is connected.
    binding: Arc<PlatformDesiredState>,
}

impl PlatformManagementTarget {
    /// Builds a target over the binding it drives.
    #[must_use]
    pub fn new(
        api_base_url: String,
        http_timeout_seconds: u64,
        clock: Arc<dyn Clock>,
        binding: Arc<PlatformDesiredState>,
    ) -> Self {
        Self {
            api_base_url,
            http_timeout_seconds,
            clock,
            binding,
        }
    }
}

#[async_trait::async_trait]
impl IntegrationTarget for PlatformManagementTarget {
    async fn bind(&self, integration: &GitIntegration, private_key: &SecretValue) -> Result<(), String> {
        let repository = integration
            .repository()
            .ok_or_else(|| "the integration has not settled on a repository".to_owned())?;

        let installation = integration
            .installation
            .as_ref()
            .ok_or_else(|| "the application has not been installed".to_owned())?;

        // The App's own installation, and no other. There is no path by which
        // client configuration's installation could authorise this: the
        // identifiers come from *this* integration's record, which the keyed
        // store reads from a path only platform management writes.
        let credential = GitCredential::app(&integration.app_id, &installation.id, private_key.expose());

        let platform = PlatformGitRepository::new(
            &PlatformRepositoryConfig {
                api_base_url: self.api_base_url.clone(),
                owner: repository.owner.clone(),
                repository: repository.name.clone(),
                branch: repository.branch.clone(),
                http_timeout_seconds: self.http_timeout_seconds,
            },
            credential,
            Arc::clone(&self.clock),
        )?;

        // Awaited, and that is the point of the whole signature: the binding
        // does not swap until every operation already running against the
        // repository it is replacing has finished.
        self.binding
            .connect(Arc::new(platform) as Arc<dyn DesiredState>)
            .await;

        Ok(())
    }

    async fn unbind(&self) {
        self.binding.disconnect().await;
    }

    async fn unusable(&self, detail: &str) {
        self.binding.unusable(detail).await;
    }
}
