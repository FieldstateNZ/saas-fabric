//! One client's secrets, inside that client's namespace.
//!
//! # The namespace is the boundary, and the store enforces it
//!
//! Every request carries `X-Vault-Namespace`. It is a header rather than a
//! path segment this code assembles, which is what makes the boundary the
//! store's to enforce rather than this adapter's to remember. Measured
//! against a real store: the same path read without the header, and read in
//! another client's namespace, both answer `404`.
//!
//! # Why versions are carried through rather than smoothed away
//!
//! The store speaks key-value version 2, and this adapter keeps its
//! `current_version` and its check-and-set. Flattening them would turn a
//! concurrent write into a silent overwrite; keeping them turns it into a
//! refusal an operator can see (ADR 0008's habit, applied to secrets).

mod listing;
mod operations;
mod wire;

use std::sync::Arc;

use fabric_control_plane::SecretPath;

use crate::OpenBao;

/// One client's secrets, read and written through the platform's store.
pub struct OpenBaoClientSecrets {
    /// The store, and the platform's own credential for it.
    store: Arc<OpenBao>,
}

impl OpenBaoClientSecrets {
    /// Builds an adapter over a store client.
    #[must_use]
    pub const fn new(store: Arc<OpenBao>) -> Self {
        Self { store }
    }

    /// Where a secret's values live inside a client's namespace.
    fn data_url(&self, path: &SecretPath) -> String {
        format!("{}/v1/{}/data/{path}", self.store.address(), self.store.mount())
    }

    /// Where a secret's metadata lives inside a client's namespace.
    fn metadata_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/metadata/{path}",
            self.store.address(),
            self.store.mount()
        )
    }

    /// One request inside a client's namespace.
    async fn send(
        &self,
        namespace: &fabric_control_plane::SecretNamespace,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, fabric_control_plane::SecretsError> {
        self.store
            .send_in(namespace.as_str(), method, url, body)
            .await
            .map_err(|_| fabric_control_plane::SecretsError::Unavailable)
    }

    /// The entries the store lists directly under a prefix.
    pub(super) async fn entries_at(
        &self,
        namespace: &fabric_control_plane::SecretNamespace,
        prefix: &str,
    ) -> Result<Vec<String>, fabric_control_plane::SecretsError> {
        let url = self.metadata_url(prefix.trim_end_matches('/'));
        let method = reqwest::Method::from_bytes(b"LIST").unwrap_or(reqwest::Method::GET);

        let response = self.send(namespace, method, &url, None).await?;

        // An empty prefix is the ordinary state of a client with no secrets
        // yet, and the store answers it with a 404. That is not a failure to
        // report to an operator opening the tab for the first time.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }

        let body = wire::body(response).await?;

        Ok(body
            .pointer("/data/keys")
            .and_then(serde_json::Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(|key| key.as_str().map(std::borrow::ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}
