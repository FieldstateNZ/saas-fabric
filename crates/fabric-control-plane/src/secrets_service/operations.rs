//! The five things an operator can do to a client's secrets.
//!
//! Each resolves the boundary first and records the outcome after, so neither
//! is something a new operation can forget to do.

use fabric_client_model::ClientId;

use super::{outcome, SecretsService};
use crate::{audit, ControlPlaneError, Operator, SecretMetadata, SecretPath, SecretValues};

impl SecretsService {
    /// Every secret path this client has.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the client is unknown, has no
    /// boundary, or the store could not be read.
    pub async fn list(&self, client: &ClientId) -> Result<Vec<SecretPath>, ControlPlaneError> {
        let boundary = self.boundary(client).await?;

        Ok(self.store.list(&boundary).await?)
    }

    /// What is known about one secret without revealing it.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the client, boundary or secret is
    /// absent, or the store could not be read.
    pub async fn metadata(
        &self,
        client: &ClientId,
        path: &SecretPath,
    ) -> Result<SecretMetadata, ControlPlaneError> {
        let boundary = self.boundary(client).await?;

        Ok(self.store.metadata(&boundary, path).await?)
    }

    /// The values, because somebody deliberately asked.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the client, boundary or secret is
    /// absent, or the store could not be read.
    pub async fn reveal(
        &self,
        operator: &Operator,
        client: &ClientId,
        path: &SecretPath,
    ) -> Result<SecretValues, ControlPlaneError> {
        let boundary = self.boundary(client).await?;
        let revealed = self.store.reveal(&boundary, path).await;

        audit::client_secret(
            operator,
            client,
            path,
            "reveal_client_secret",
            outcome(&revealed),
            None,
        );

        Ok(revealed?)
    }

    /// Writes a secret, refusing a write against a superseded version.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] on a version conflict, or if the client,
    /// boundary or store is unavailable.
    pub async fn write(
        &self,
        operator: &Operator,
        client: &ClientId,
        path: &SecretPath,
        values: &SecretValues,
        expected: Option<u64>,
    ) -> Result<u64, ControlPlaneError> {
        let boundary = self.boundary(client).await?;
        let written = self.store.write(&boundary, path, values, expected).await;

        audit::client_secret(
            operator,
            client,
            path,
            "write_client_secret",
            outcome(&written),
            written.as_ref().ok().copied(),
        );

        Ok(written?)
    }

    /// Removes a secret and every version of it.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the client, boundary or secret is
    /// absent, or the store could not be reached.
    pub async fn delete(
        &self,
        operator: &Operator,
        client: &ClientId,
        path: &SecretPath,
    ) -> Result<(), ControlPlaneError> {
        let boundary = self.boundary(client).await?;
        let removed = self.store.delete(&boundary, path).await;

        audit::client_secret(
            operator,
            client,
            path,
            "delete_client_secret",
            outcome(&removed),
            None,
        );

        Ok(removed?)
    }
}
