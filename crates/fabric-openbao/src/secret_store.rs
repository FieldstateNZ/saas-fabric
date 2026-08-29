//! This instance's secret partition, kept in OpenBao.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_control_plane::{SecretName, SecretStore, SecretStoreError, SecretValue};

use crate::client::OpenBao;
use crate::kv::Read;

/// The field a secret's value is written under.
///
/// One field per entry, always called the same thing. A store entry that held
/// several named values would invite callers to reach for one of the others,
/// and the port deliberately offers no way to ask for part of a secret.
const VALUE: &str = "value";

/// Secrets for one Fabric instance.
pub struct OpenBaoSecretStore {
    /// The client, shared with the integration store so one login serves both.
    client: Arc<OpenBao>,
}

impl OpenBaoSecretStore {
    /// Builds a store over a client.
    #[must_use]
    pub fn new(client: Arc<OpenBao>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SecretStore for OpenBaoSecretStore {
    async fn get(&self, name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError> {
        match self
            .client
            .read(name.as_str())
            .await
            .map_err(|error| classify(&error))?
        {
            Read::Absent => Ok(None),
            Read::Found(fields) => fields
                .get(VALUE)
                .and_then(serde_json::Value::as_str)
                .map(|value| Some(SecretValue::new(value)))
                .ok_or(SecretStoreError::Malformed),
        }
    }

    async fn put(&self, name: &SecretName, value: &SecretValue) -> Result<(), SecretStoreError> {
        self.client
            .write(name.as_str(), serde_json::json!({ VALUE: value.expose() }))
            .await
            .map_err(|error| classify(&error))
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        self.client
            .remove(name.as_str())
            .await
            .map_err(|error| classify(&error))
    }

    fn describe(&self) -> String {
        self.client.describe()
    }
}

/// Turns a store failure into the port's vocabulary.
///
/// The message is inspected rather than a status, because the client has
/// already reduced every path to a sentence. What matters to a caller is only
/// whether the platform was refused — which needs somebody to widen a policy —
/// or could not reach the store, which usually needs nothing.
pub(crate) fn classify(error: &str) -> SecretStoreError {
    if error.contains("refused the login") || error.contains("403") {
        SecretStoreError::NotPermitted
    } else if error.contains("could not be read") || error.contains("no entry") {
        SecretStoreError::Malformed
    } else {
        SecretStoreError::Unavailable
    }
}
