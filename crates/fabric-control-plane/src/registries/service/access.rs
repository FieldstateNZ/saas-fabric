//! What every change needs: the record for a host, its credential, a client
//! built for it, and the deployment's rule.

use std::sync::Arc;

use fabric_component::Repository;

use crate::registries::endpoint::same;
use crate::registries::logging::credential_left_behind;
use crate::registries::record::SecretId;
use crate::registries::service::Inner;
use crate::registries::{
    RealmOrigin, RegistryClient, RegistryConnection, RegistryCredential, RegistryFailure, RegistryHost,
    RegistryRecord,
};

impl Inner {
    /// The record for `host` in `records`.
    pub(super) fn find<'a>(
        records: &'a mut [RegistryRecord],
        host: &RegistryHost,
    ) -> Result<&'a mut RegistryRecord, RegistryFailure> {
        records
            .iter_mut()
            .find(|record| &record.host == host)
            .ok_or_else(|| RegistryFailure::NotFound {
                host: host.to_string(),
            })
    }

    /// The credential `record` holds, read back from the secret partition,
    /// carrying its refusal mark.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::CredentialUnreadable`] if the record names a
    /// credential that is not there — a store that lost it, not a registry
    /// without one, and no retry brings it back;
    /// [`RegistryFailure::StoreUnavailable`] if it could not be read now.
    pub(super) async fn credential_of(
        &self,
        record: &RegistryRecord,
    ) -> Result<Option<RegistryCredential>, RegistryFailure> {
        let Some(held) = &record.credential else {
            return Ok(None);
        };
        match self.secrets.get(&record.secret_id.credential()).await {
            Ok(Some(token)) => Ok(Some(
                self.marked(&record.secret_id, RegistryCredential::new(&held.username, token)),
            )),
            Ok(None) => Err(RegistryFailure::CredentialUnreadable {
                host: record.host.to_string(),
            }),
            Err(_) => Err(RegistryFailure::StoreUnavailable),
        }
    }

    /// A client for a connection.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::Invalid`] with the adapter's message, which names
    /// a field and never its value.
    pub(super) fn connect(
        &self,
        connection: RegistryConnection,
    ) -> Result<Arc<dyn RegistryClient>, RegistryFailure> {
        self.connector
            .connect(connection)
            .map_err(RegistryFailure::Invalid)
    }

    /// Refuses a registration for the deployment's host at any endpoint but
    /// the deployment's.
    pub(super) fn check_deployment(
        &self,
        host: &RegistryHost,
        endpoint: &str,
    ) -> Result<(), RegistryFailure> {
        match &self.deployment {
            Some(deployment) if deployment.host == host.as_str() && !same(&deployment.endpoint, endpoint) => {
                Err(RegistryFailure::EndpointDiffers {
                    host: host.to_string(),
                })
            }
            _ => Ok(()),
        }
    }

    /// Deletes a credential no record names any more, and its mark. Its
    /// failure is logged and not returned: the change that stopped naming it
    /// already stands.
    pub(super) async fn forget(&self, host: &RegistryHost, secret: &SecretId) {
        self.drop_mark(secret);
        if self.secrets.delete(&secret.credential()).await.is_err() {
            credential_left_behind(host);
        }
    }
}

impl RegistryRecord {
    /// How a registered registry is connected: at the realm it recorded,
    /// with `credential` presented for `repositories`.
    pub(crate) fn connection(
        &self,
        credential: Option<RegistryCredential>,
        repositories: Vec<Repository>,
    ) -> RegistryConnection {
        RegistryConnection {
            host: self.host.clone(),
            kind: self.kind,
            endpoint: self.endpoint.clone(),
            realm: RealmOrigin::Recorded(self.realm_origin.clone()),
            credential,
            repositories,
        }
    }
}
