//! This instance's Git integration record, kept in OpenBao.
//!
//! # Why the record shares a store with the secrets
//!
//! Not because they are the same kind of thing — the ports are separate for
//! exactly that reason — but because this platform has one durable store and
//! introducing a second backing service to hold a dozen non-secret fields
//! would be a service to run, secure and back up for no benefit.
//!
//! What keeps them honest is the type, not the location: a caller holding an
//! integration record cannot reach a secret through it, and the record is the
//! only one of the two the API is allowed to describe to an operator.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_control_plane::{GitIntegration, IntegrationKind, IntegrationStore, IntegrationStoreError};

use crate::client::OpenBao;
use crate::kv::Read;
use crate::secret_store::classify;
use fabric_control_plane::SecretStoreError;

/// Where an integration's record lives within the instance's partition.
///
/// # The client path is not moving
///
/// `git/integration` is where a connected instance keeps its record today, and
/// it stays there. Relocating a live integration to make the two paths look
/// alike would be a migration whose upside is symmetry and whose downside is a
/// platform that has forgotten how to reach client configuration.
///
/// The asymmetry records which one came first. That is what happened.
const fn record(kind: IntegrationKind) -> &'static str {
    match kind {
        IntegrationKind::ClientConfiguration => "git/integration",
        IntegrationKind::PlatformManagement => "integrations/platform-management/integration",
    }
}

/// The field the record is written under.
const DOCUMENT: &str = "record";

/// The Git integration record for one Fabric instance.
pub struct OpenBaoIntegrationStore {
    /// The client, shared with the secret store so one login serves both.
    client: Arc<OpenBao>,
}

impl OpenBaoIntegrationStore {
    /// Builds a store over a client.
    #[must_use]
    pub fn new(client: Arc<OpenBao>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl IntegrationStore for OpenBaoIntegrationStore {
    async fn load(&self, kind: IntegrationKind) -> Result<Option<GitIntegration>, IntegrationStoreError> {
        let fields = match self
            .client
            .read(record(kind))
            .await
            .map_err(|error| translate(&error))?
        {
            Read::Absent => return Ok(None),
            Read::Found(fields) => fields,
        };

        let document = fields
            .get(DOCUMENT)
            .and_then(serde_json::Value::as_str)
            .ok_or(IntegrationStoreError::Malformed)?;

        serde_json::from_str(document)
            .map(Some)
            .map_err(|_| IntegrationStoreError::Malformed)
    }

    async fn save(
        &self,
        kind: IntegrationKind,
        integration: &GitIntegration,
    ) -> Result<(), IntegrationStoreError> {
        let document = serde_json::to_string(integration).map_err(|_| IntegrationStoreError::Malformed)?;

        self.client
            .write(record(kind), serde_json::json!({ DOCUMENT: document }))
            .await
            .map_err(|error| translate(&error))
    }

    async fn clear(&self, kind: IntegrationKind) -> Result<(), IntegrationStoreError> {
        self.client
            .remove(record(kind))
            .await
            .map_err(|error| translate(&error))
    }
}

/// Turns a store failure into this port's vocabulary.
fn translate(error: &str) -> IntegrationStoreError {
    match classify(error) {
        SecretStoreError::NotPermitted => IntegrationStoreError::NotPermitted,
        SecretStoreError::Malformed => IntegrationStoreError::Malformed,
        SecretStoreError::Unavailable | SecretStoreError::ReadOnly => IntegrationStoreError::Unavailable,
    }
}
