//! Registering a registry: resolved by its kind, proven, then recorded.
//!
//! Kept in one file because proving, writing and installing a registration
//! are one ordered change, and the order is the point.

use std::sync::Arc;

use crate::audit::RegistryOperation;
use crate::registries::record::{CredentialRecord, SecretId};
use crate::registries::service::resolve::{resolve, Resolved};
use crate::registries::service::{Inner, Listed, Live, Registration, RegistryService};
use crate::registries::{RealmOrigin, RegistryConnection, RegistryFailure, RegistryRecord};
use crate::Operator;

impl RegistryService {
    /// Registers a registry: `ghcr` and `dockerHub` where their kind fixes,
    /// `distribution` at the HTTPS origin given, with a credential or none.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::Invalid`] for a registration the rules refuse,
    /// [`RegistryFailure::Exists`] if one is registered for the host,
    /// [`RegistryFailure::EndpointDiffers`] for the deployment's host at
    /// another endpoint, and a proof's failure if it did not prove.
    pub(crate) async fn register(
        &self,
        operator: &Operator,
        registration: Registration,
    ) -> Result<Listed, RegistryFailure> {
        let operation = RegistryOperation::Register;
        let resolved = match resolve(registration) {
            Ok(resolved) => resolved,
            Err(failure) => return Self::refused(operator, None, operation, failure),
        };
        let subject = operator.subject().to_owned();
        let host = resolved.host.clone();

        self.change(operator, Some(host), operation, move |inner| {
            register(inner, subject, resolved)
        })
        .await
    }
}

/// Proves, then records credential first and record second, then installs.
async fn register(
    inner: Arc<Inner>,
    operator: String,
    resolved: Resolved,
) -> Result<Listed, RegistryFailure> {
    let Resolved {
        host,
        kind,
        endpoint,
        credential,
    } = resolved;
    let mut records = inner.store.load().await?;
    if records.iter().any(|record| record.host == host) {
        return Err(RegistryFailure::Exists {
            host: host.to_string(),
        });
    }
    inner.check_deployment(&host, &endpoint)?;

    let proving = inner.connect(RegistryConnection {
        host: host.clone(),
        kind,
        endpoint: endpoint.clone(),
        realm: RealmOrigin::Follow,
        credential: credential.clone(),
        repositories: Vec::new(),
    })?;
    let realm_origin = proving.prove().await.map_err(RegistryFailure::proving_registry)?;

    let secret_id = SecretId::mint().map_err(RegistryFailure::Unavailable)?;
    let now = inner.clock.now_unix_seconds();
    if let Some(credential) = &credential {
        inner
            .secrets
            .put(&secret_id.credential(), credential.token())
            .await
            .map_err(|_| RegistryFailure::StoreUnavailable)?;
    }

    let record = RegistryRecord {
        host: host.clone(),
        kind,
        endpoint,
        realm_origin,
        secret_id,
        credential: credential.as_ref().map(|credential| CredentialRecord {
            username: credential.username().to_owned(),
            set_by: operator.clone(),
            set_at: now,
        }),
        registered_by: operator,
        registered_at: now,
        repositories: Vec::new(),
    };
    records.push(record.clone());
    if let Err(error) = inner.store.save(&records).await {
        if credential.is_some() {
            inner.forget(&host, &record.secret_id).await;
        }
        return Err(error.into());
    }

    // Read through at the realm it recorded, as it will be after a restart;
    // the client that proved it stands in only if that cannot be built.
    if let Some(credential) = &credential {
        inner.hold_mark(&record.secret_id, credential);
    }
    let client = inner
        .connect(record.connection(credential, Vec::new()))
        .unwrap_or(proving);
    inner.swap(
        &host,
        Some(Live {
            client,
            credential_unreadable: false,
        }),
    );
    Ok(inner.listed(record))
}
