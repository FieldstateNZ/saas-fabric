//! The operations a client's secrets support, each inside that client's
//! namespace.

use async_trait::async_trait;
use fabric_control_plane::{
    ClientSecrets, SecretMetadata, SecretNamespace, SecretPath, SecretValues, SecretsError,
};

use super::{wire, OpenBaoClientSecrets};

#[async_trait]
impl ClientSecrets for OpenBaoClientSecrets {
    async fn list(&self, namespace: &SecretNamespace) -> Result<Vec<SecretPath>, SecretsError> {
        super::listing::walk(self, namespace).await
    }

    async fn metadata(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
    ) -> Result<SecretMetadata, SecretsError> {
        let response = self
            .send(
                namespace,
                reqwest::Method::GET,
                &self.metadata_url(path.as_str()),
                None,
            )
            .await?;

        wire::metadata(&wire::body(response).await?)
    }

    async fn reveal(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
    ) -> Result<SecretValues, SecretsError> {
        let response = self
            .send(namespace, reqwest::Method::GET, &self.data_url(path), None)
            .await?;

        wire::values(&wire::body(response).await?)
    }

    async fn write(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
        values: &SecretValues,
        expected: Option<u64>,
    ) -> Result<u64, SecretsError> {
        // `cas` is always sent. Omitting it lets the store accept a write
        // against a version somebody has already moved past, which is the
        // silent overwrite this whole shape exists to prevent. `None` means
        // "I believe this does not exist yet", which the store spells `0`.
        let body = serde_json::json!({
            "data": values.revealed(),
            "options": { "cas": expected.unwrap_or(0) },
        });

        let response = self
            .send(namespace, reqwest::Method::POST, &self.data_url(path), Some(body))
            .await?;

        wire::written(response).await
    }

    async fn delete(&self, namespace: &SecretNamespace, path: &SecretPath) -> Result<(), SecretsError> {
        // The metadata endpoint, which removes every version. Deleting through
        // the data endpoint marks the newest version deleted and leaves the
        // older ones readable, which is not what an operator pressing Delete
        // means.
        let response = self
            .send(
                namespace,
                reqwest::Method::DELETE,
                &self.metadata_url(path.as_str()),
                None,
            )
            .await?;

        wire::removed(&response)
    }
}
