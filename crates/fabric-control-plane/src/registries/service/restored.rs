//! Restoring one recorded registry, and installing the restored set.

use std::collections::BTreeMap;

use crate::registries::endpoint::same;
use crate::registries::logging::restore_incomplete;
use crate::registries::service::{Inner, Live};
use crate::registries::{RegistryCredential, RegistryHost, RegistryRecord};

impl Inner {
    /// The client one record is read through, or `None` if it is not; and
    /// whether its credential, if it holds one, could be asked for.
    pub(super) async fn restored(&self, record: &RegistryRecord) -> (Option<Live>, bool) {
        let host = &record.host;
        if let Some(deployment) = &self.deployment {
            if deployment.host == host.as_str() && !same(&deployment.endpoint, &record.endpoint) {
                restore_incomplete(
                    Some(host),
                    "recorded for the deployment's host at an endpoint that is no longer the deployment's",
                );
                return (None, true);
            }
        }

        let (credential, credential_unreadable, read) = match &record.credential {
            None => (None, false, true),
            Some(held) => match self.secrets.get(&record.secret_id.credential()).await {
                Ok(Some(token)) => {
                    let credential = RegistryCredential::new(&held.username, token);
                    (Some(self.marked(&record.secret_id, credential)), false, true)
                }
                Ok(None) => {
                    restore_incomplete(
                        Some(host),
                        "its credential is not in the secret store; it is read anonymously until the credential is set again",
                    );
                    (None, true, true)
                }
                Err(_) => {
                    restore_incomplete(
                        Some(host),
                        "its credential could not be read now; it is read anonymously until the store answers",
                    );
                    (None, true, false)
                }
            },
        };

        let live = match self
            .connector
            .connect(record.connection(credential, record.repository_names()))
        {
            Ok(client) => Some(Live {
                client,
                credential_unreadable,
            }),
            Err(detail) => {
                restore_incomplete(Some(host), &detail);
                None
            }
        };
        (live, read)
    }

    /// Replaces the whole live set, and installs it.
    pub(super) fn swap_all(&self, live: BTreeMap<RegistryHost, Live>) {
        let clients = live
            .iter()
            .map(|(host, entry)| (host.clone(), entry.client.clone()))
            .collect();
        *self.live() = live;
        self.connector.install(clients);
    }
}
