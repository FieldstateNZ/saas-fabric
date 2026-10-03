//! This instance's image registry records, kept in OpenBao beside the Git
//! integrations' (ADR 0026 section 5).
//!
//! # Why here, and why one entry
//!
//! Registries are operator-managed integration state, and the instance
//! partition is where this platform keeps that — the exception to ADR 0008
//! that ADR 0011 made for the Git integrations, and ADR 0026 states again: a
//! credential cannot live in desired state, and a registry that depended on
//! the client-configuration repository could not be registered before it.
//!
//! The whole set is one entry, `integrations/registries`, with the set in its
//! `record` field: the partition offers no listing, and there are a handful.
//! Each credential is its own secret beneath it, written through
//! [`OpenBaoSecretStore`](crate::OpenBaoSecretStore) under a name the control
//! plane minted; nothing here ever sees one.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_control_plane::{RegistryRecord, RegistryStore, RegistryStoreError, SecretStoreError};

use crate::client::OpenBao;
use crate::kv::Read;
use crate::secret_store::classify;

/// Where the record set lives within the instance's partition.
const RECORDS: &str = "integrations/registries";

/// The field the record set is written under, as the Git integration's is.
const DOCUMENT: &str = "record";

/// The image registry records for one Fabric instance.
pub struct OpenBaoRegistryStore {
    /// The client, shared with the other stores so one login serves them all.
    client: Arc<OpenBao>,
}

impl OpenBaoRegistryStore {
    /// Builds a store over a client.
    #[must_use]
    pub fn new(client: Arc<OpenBao>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RegistryStore for OpenBaoRegistryStore {
    async fn load(&self) -> Result<Vec<RegistryRecord>, RegistryStoreError> {
        let fields = match self
            .client
            .read(RECORDS)
            .await
            .map_err(|error| translate(&error))?
        {
            Read::Absent => return Ok(Vec::new()),
            Read::Found(fields) => fields,
        };

        let document = fields
            .get(DOCUMENT)
            .and_then(serde_json::Value::as_str)
            .ok_or(RegistryStoreError::Malformed)?;

        serde_json::from_str(document).map_err(|_| RegistryStoreError::Malformed)
    }

    async fn save(&self, records: &[RegistryRecord]) -> Result<(), RegistryStoreError> {
        let document = serde_json::to_string(records).map_err(|_| RegistryStoreError::Malformed)?;

        self.client
            .write(RECORDS, serde_json::json!({ DOCUMENT: document }))
            .await
            .map_err(|error| translate(&error))
    }
}

/// Turns a store failure into this port's vocabulary.
fn translate(error: &str) -> RegistryStoreError {
    match classify(error) {
        SecretStoreError::NotPermitted => RegistryStoreError::NotPermitted,
        SecretStoreError::Malformed => RegistryStoreError::Malformed,
        SecretStoreError::Unavailable | SecretStoreError::ReadOnly => RegistryStoreError::Unavailable,
    }
}
