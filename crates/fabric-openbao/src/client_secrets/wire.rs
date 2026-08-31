//! Reading the store's answers, and nothing more of them than is needed.

use std::collections::BTreeMap;

use fabric_control_plane::{SecretMetadata, SecretValues, SecretsError};

/// Turns a response into a body, mapping the statuses that mean something.
pub(super) async fn body(response: reqwest::Response) -> Result<serde_json::Value, SecretsError> {
    match response.status() {
        status if status.is_success() => response.json().await.map_err(|_| SecretsError::Unavailable),

        reqwest::StatusCode::NOT_FOUND => Err(SecretsError::NotFound),

        // The platform's own credential was refused, so this is a policy
        // problem here rather than anything the operator did.
        reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::UNAUTHORIZED => Err(SecretsError::Refused),

        _ => Err(SecretsError::Unavailable),
    }
}

/// The version and timestamp, with no values read.
pub(super) fn metadata(body: &serde_json::Value) -> Result<SecretMetadata, SecretsError> {
    let data = body.get("data").ok_or(SecretsError::Unavailable)?;

    Ok(SecretMetadata {
        version: data
            .get("current_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(SecretsError::Unavailable)?,
        updated_at: data
            .get("updated_time")
            .and_then(serde_json::Value::as_str)
            .map(std::borrow::ToOwned::to_owned),
    })
}

/// The values, from a read of the data endpoint.
pub(super) fn values(body: &serde_json::Value) -> Result<SecretValues, SecretsError> {
    let fields = body
        .pointer("/data/data")
        .and_then(serde_json::Value::as_object)
        .ok_or(SecretsError::Unavailable)?;

    let mut values = BTreeMap::new();

    for (key, value) in fields {
        // Only strings. A nested object would have to be rendered somehow, and
        // every rendering is a way for a secret to end up somewhere it is not
        // expected — the console shows and edits strings.
        let text = value.as_str().ok_or(SecretsError::Unavailable)?;

        values.insert(key.clone(), text.to_owned());
    }

    Ok(SecretValues::new(values))
}

/// The version a write produced, or the conflict it hit.
pub(super) async fn written(response: reqwest::Response) -> Result<u64, SecretsError> {
    // A check-and-set mismatch is the store's `400`, and it is the one caller
    // error that is genuinely the operator's to resolve: somebody else wrote
    // while they were looking. It must not arrive as a generic failure.
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        return Err(SecretsError::Conflict);
    }

    let body = body(response).await?;

    body.pointer("/data/version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(SecretsError::Unavailable)
}

/// Whether a delete took effect.
pub(super) fn removed(response: &reqwest::Response) -> Result<(), SecretsError> {
    match response.status() {
        status if status.is_success() => Ok(()),
        reqwest::StatusCode::NOT_FOUND => Err(SecretsError::NotFound),
        reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::UNAUTHORIZED => Err(SecretsError::Refused),
        _ => Err(SecretsError::Unavailable),
    }
}
