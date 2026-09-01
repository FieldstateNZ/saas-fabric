//! Creating this platform's own application on the Git host, and installing it.
//!
//! # Why the platform creates its own application
//!
//! The alternative is a human creating one by hand, copying a private key into
//! a secret store, and writing two identifiers into a deployment before the
//! platform can start. That made onboarding somebody else's job and made the
//! credential a thing people handled.
//!
//! GitHub's App Manifest flow removes all of it: the platform describes the
//! application it wants, the operator approves it in their organisation, and
//! the host hands back the identity — including a private key that **arrives
//! exactly once**.
//!
//! # No webhooks, deliberately
//!
//! The manifest asks for no events and declares no hook. SaaS Fabric's control
//! plane is published on the operator plane and on no public one, so GitHub's
//! servers cannot reach it and a hook would be a URL that never delivers.
//!
//! The consequence is stated rather than hidden: the platform is not *told*
//! when an installation is revoked, so it finds out on its next sweep. That is
//! why integration health is probed rather than remembered.

mod conversion;
mod installation;
mod manifest;
#[cfg(test)]
mod manifest_tests;
mod purpose;

pub use purpose::AppPurpose;

use async_trait::async_trait;
use fabric_control_plane::{
    AppCreationRequest, CreatedApp, GitAppProvisioning, InstallationDetail, ProvisioningError, SecretValue,
};

/// Creates and inspects this platform's application on GitHub.
pub struct GitHubAppProvisioning {
    /// Where the API lives, so an enterprise host can be pointed at.
    api_base_url: String,

    /// Where the website lives, which is where a browser is sent.
    web_base_url: String,

    /// The externally reachable base the host returns the browser to.
    callback_base_url: String,

    /// What the application it creates is for, which decides the application's
    /// name and where its callbacks land.
    purpose: AppPurpose,

    /// The HTTP client.
    http: reqwest::Client,
}

impl GitHubAppProvisioning {
    /// Builds a provisioner.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(
        api_base_url: &str,
        web_base_url: &str,
        callback_base_url: &str,
        timeout: std::time::Duration,
        purpose: AppPurpose,
    ) -> Result<Self, String> {
        Ok(Self {
            api_base_url: api_base_url.trim_end_matches('/').to_owned(),
            web_base_url: web_base_url.trim_end_matches('/').to_owned(),
            callback_base_url: callback_base_url.trim_end_matches('/').to_owned(),
            purpose,
            http: reqwest::Client::builder()
                .timeout(timeout)
                .user_agent("saas-fabric-control-plane")
                .build()
                .map_err(|error| format!("git provisioning: {error}"))?,
        })
    }
}

/// Percent-encodes a value being interpolated into a path.
///
/// Shared by the steps below. Conservative on purpose — everything outside RFC
/// 3986's unreserved set — because the values here come from a Git host and
/// from a browser's query string, and the failure mode of an allowlist
/// somebody widens later is a request aimed somewhere else.
pub(crate) fn urlencode_path(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[async_trait]
impl GitAppProvisioning for GitHubAppProvisioning {
    fn creation_request(&self, organisation: &str, state: &str) -> AppCreationRequest {
        AppCreationRequest {
            post_url: manifest::creation_url(&self.web_base_url, organisation, state),
            manifest: manifest::build(&self.callback_base_url, &self.purpose),
        }
    }

    async fn redeem_creation(&self, code: &str) -> Result<CreatedApp, ProvisioningError> {
        conversion::redeem(&self.http, &self.api_base_url, code).await
    }

    fn install_url(&self, app_slug: &str, state: &str) -> String {
        manifest::install_url(&self.web_base_url, app_slug, state)
    }

    async fn inspect_installation(
        &self,
        app_id: &str,
        private_key: &SecretValue,
        installation_id: &str,
    ) -> Result<InstallationDetail, ProvisioningError> {
        installation::inspect(
            &self.http,
            &self.api_base_url,
            app_id,
            private_key,
            installation_id,
        )
        .await
    }
}
