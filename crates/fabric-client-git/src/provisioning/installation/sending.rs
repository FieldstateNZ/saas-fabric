//! How a provisioning request is made.
//!
//! Split from the steps because they are different concerns: the steps decide
//! *what* to ask the host, and this decides how every one of those asks is
//! shaped and how its status is read.

use fabric_control_plane::ProvisioningError;

/// A GET carrying a bearer, decoded into `T`.
pub(super) async fn get<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    bearer: &str,
) -> Result<T, ProvisioningError> {
    decode(http.get(url), bearer).await
}

/// A POST carrying a bearer, decoded into `T`.
pub(super) async fn post<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    bearer: &str,
) -> Result<T, ProvisioningError> {
    decode(http.post(url), bearer).await
}

/// Sends a prepared request and decodes a successful response.
///
/// A `4xx` becomes [`ProvisioningError::Refused`] and everything else
/// [`Unavailable`](ProvisioningError::Unavailable), because the two lead
/// somewhere different: one needs the operator to redo the flow and the other
/// will probably work shortly. The host's own body is never read into the
/// error — this reaches a browser, and an upstream body is not something to
/// reflect unread.
async fn decode<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    bearer: &str,
) -> Result<T, ProvisioningError> {
    let response = request
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|_| ProvisioningError::Unavailable)?;

    if !response.status().is_success() {
        return Err(if response.status().is_client_error() {
            ProvisioningError::Refused
        } else {
            ProvisioningError::Unavailable
        });
    }

    response.json().await.map_err(|_| ProvisioningError::Unavailable)
}
