//! Creating and installing the platform's own Git host application.
//!
//! # The port is deliberately four narrow methods
//!
//! Not "a GitHub client". The control plane orchestrates a flow — start,
//! redeem, install, verify — and every host-specific fact about how that is
//! done lives behind this. Nothing here names an endpoint, a manifest field or
//! a header.

use async_trait::async_trait;
use serde::Serialize;

use crate::git_integration::{GitIntegration, SecretValue};

/// What the browser must send to the Git host to create the application.
///
/// The manifest is carried as opaque JSON on purpose. Its fields are the
/// host's vocabulary, and a domain that named them would be a domain that has
/// to change when the host adds one.
#[derive(Debug, Clone, Serialize)]
pub struct AppCreationRequest {
    /// Where the browser posts the manifest.
    pub post_url: String,

    /// The manifest itself, to be posted as a form field.
    pub manifest: serde_json::Value,
}

/// An application the host has just created for this platform.
#[derive(Debug)]
pub struct CreatedApp {
    /// The application's identifier.
    pub app_id: String,

    /// Its URL slug, needed to build the install page.
    pub app_slug: String,

    /// Its private key. **Arrives exactly once**, in the response to the
    /// redemption below; if it is not stored then it is gone and the
    /// application has to be created again.
    pub private_key: SecretValue,
}

/// A repository an installation can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibleRepository {
    /// The owning account or organisation.
    pub owner: String,

    /// The repository name.
    pub name: String,

    /// The branch the host considers its default.
    pub default_branch: String,
}

/// What an installation turned out to be.
#[derive(Debug)]
pub struct InstallationDetail {
    /// The account the application is installed on.
    pub account: String,

    /// Every repository the installation can reach.
    pub repositories: Vec<AccessibleRepository>,
}

/// Why a provisioning step could not be completed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProvisioningError {
    /// The host refused the request.
    ///
    /// A spent or expired redemption code, or a key it will not accept. No
    /// retry helps and the host's own message is not repeated to a browser.
    #[error("the Git host refused the request")]
    Refused,

    /// The host could not be reached, or answered unintelligibly.
    #[error("the Git host is unavailable")]
    Unavailable,
}

/// Builds a desired-state repository from an established integration.
///
/// Separate from [`GitAppProvisioning`] because it is a different question
/// with a different lifetime: provisioning happens once when an operator
/// connects, and this happens every time the platform binds — at startup, and
/// again whenever the integration changes.
pub trait DesiredStateFactory: Send + Sync {
    /// Builds a repository for this integration.
    ///
    /// # Errors
    ///
    /// Returns a message if the integration does not describe a usable
    /// repository, or if a client cannot be built for it.
    fn build(
        &self,
        integration: &GitIntegration,
        private_key: &SecretValue,
    ) -> Result<std::sync::Arc<dyn crate::ClientRepository>, String>;
}

/// Creates the platform's application on a Git host, and inspects it.
#[async_trait]
pub trait GitAppProvisioning: Send + Sync {
    /// What the browser must post to create the application.
    ///
    /// `organisation` is where it will be created. `state` is the platform's
    /// correlation token, which the host returns verbatim to the callback —
    /// the adapter decides where in the request it belongs.
    fn creation_request(&self, organisation: &str, state: &str) -> AppCreationRequest;

    /// Exchanges the host's one-time creation code for the application.
    ///
    /// # Errors
    ///
    /// Returns [`ProvisioningError`] if the code was refused or the host could
    /// not be reached.
    async fn redeem_creation(&self, code: &str) -> Result<CreatedApp, ProvisioningError>;

    /// Where the browser goes to install the application.
    fn install_url(&self, app_slug: &str, state: &str) -> String;

    /// Proves the platform can act as this installation, and reports its reach.
    ///
    /// The proof matters more than the report: an installation is only
    /// recorded once a token has actually been minted for it, so a recorded
    /// installation always means a working one and no separate verified flag
    /// is needed.
    ///
    /// # Errors
    ///
    /// Returns [`ProvisioningError`] if a token could not be minted or the
    /// host could not be reached.
    async fn inspect_installation(
        &self,
        app_id: &str,
        private_key: &SecretValue,
        installation_id: &str,
    ) -> Result<InstallationDetail, ProvisioningError>;
}
