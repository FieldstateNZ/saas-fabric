//! Exchanging the host's one-time creation code for the application.

use fabric_control_plane::{CreatedApp, ProvisioningError, SecretValue};
use serde::Deserialize;

use crate::provisioning::urlencode_path;

/// What the host returns when a manifest is converted.
///
/// Four fields are returned that this platform does not keep. `client_id` and
/// `client_secret` belong to an OAuth flow SaaS Fabric does not use — it
/// authenticates as the application with the private key — and
/// `webhook_secret` verifies deliveries that will never arrive. Storing a
/// credential nothing consumes is a credential that can only leak.
#[derive(Deserialize)]
struct Converted {
    /// The application's numeric identifier.
    id: u64,

    /// Its URL slug.
    slug: String,

    /// Its private key, in PEM form. Returned exactly once.
    pem: String,
}

/// Redeems the creation code.
///
/// # Errors
///
/// Returns [`ProvisioningError::Refused`] when the host rejects the code —
/// it is single-use and short-lived, so a reload of the callback lands here —
/// and [`ProvisioningError::Unavailable`] when the host cannot be reached.
pub(super) async fn redeem(
    http: &reqwest::Client,
    api_base_url: &str,
    code: &str,
) -> Result<CreatedApp, ProvisioningError> {
    let url = format!(
        "{api_base_url}/app-manifests/{}/conversions",
        urlencode_path(code)
    );

    let response = http
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|_| ProvisioningError::Unavailable)?;

    if !response.status().is_success() {
        // A 4xx means the code is spent, expired, or was never ours. The
        // host's body is deliberately not read into the error: this failure
        // reaches a browser, and an upstream body is not something to reflect.
        return Err(if response.status().is_client_error() {
            ProvisioningError::Refused
        } else {
            ProvisioningError::Unavailable
        });
    }

    let converted: Converted = response
        .json()
        .await
        .map_err(|_| ProvisioningError::Unavailable)?;

    Ok(CreatedApp {
        app_id: converted.id.to_string(),
        app_slug: converted.slug,
        private_key: SecretValue::new(converted.pem),
    })
}
