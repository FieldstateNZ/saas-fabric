//! Setting and replacing a registry's credential.
//!
//! # A replacement is written beside the old one
//!
//! A new credential is written under a newly minted id, the record is saved
//! naming it, and only then is the old one deleted. A failure part-way leaves
//! the record naming a credential that is still there — the old one until the
//! record moves, the new one after — and never a record naming a token that
//! was half replaced.

use std::sync::Arc;

use crate::audit::RegistryOperation;
use crate::registries::record::{CredentialRecord, SecretId};
use crate::registries::service::resolve::credential;
use crate::registries::service::{Inner, Listed, Live, RegistryService};
use crate::registries::{RegistryCredential, RegistryFailure, RegistryHost};
use crate::{Operator, SecretValue};

impl RegistryService {
    /// Sets or replaces a registry's credential, once it proves: the registry
    /// with it, then every repository registered under it.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::NotFound`], [`RegistryFailure::Invalid`] for a
    /// credential the rules refuse, or a proof's failure.
    pub(crate) async fn set_credential(
        &self,
        operator: &Operator,
        host: RegistryHost,
        username: String,
        token: SecretValue,
    ) -> Result<Listed, RegistryFailure> {
        let operation = RegistryOperation::SetCredential;
        let credential = match credential(username, token) {
            Ok(credential) => credential,
            Err(failure) => return Self::refused(operator, Some(&host), operation, failure),
        };
        let subject = operator.subject().to_owned();

        self.change(operator, Some(host.clone()), operation, move |inner| {
            set(inner, subject, host, credential)
        })
        .await
    }
}

/// Proves, writes the new credential, records it, installs, then forgets the
/// old one.
async fn set(
    inner: Arc<Inner>,
    operator: String,
    host: RegistryHost,
    credential: RegistryCredential,
) -> Result<Listed, RegistryFailure> {
    let mut records = inner.store.load().await?;
    let record = Inner::find(&mut records, &host)?;
    inner.check_deployment(&host, &record.endpoint)?;
    let repositories = record.repository_names();

    let client = inner.connect(record.connection(Some(credential.clone()), repositories.clone()))?;
    client.prove().await.map_err(RegistryFailure::proving_registry)?;
    for repository in &repositories {
        let answer = client
            .prove_repository(repository)
            .await
            .map_err(RegistryFailure::proving_repository)?;
        RegistryFailure::readable(answer, repository.as_str())?;
    }

    let fresh = SecretId::mint().map_err(RegistryFailure::Unavailable)?;
    inner
        .secrets
        .put(&fresh.credential(), credential.token())
        .await
        .map_err(|_| RegistryFailure::StoreUnavailable)?;

    let now = inner.clock.now_unix_seconds();
    let had_one = record.credential.is_some();
    let previous = std::mem::replace(&mut record.secret_id, fresh.clone());
    record.credential = Some(CredentialRecord {
        username: credential.username().to_owned(),
        set_by: operator,
        set_at: now,
    });
    for registered in &mut record.repositories {
        registered.proven_at = now;
    }
    let saved = record.clone();
    if let Err(error) = inner.store.save(&records).await {
        inner.forget(&host, &fresh).await;
        return Err(error.into());
    }

    inner.hold_mark(&fresh, &credential);
    inner.swap(
        &host,
        Some(Live {
            client,
            credential_unreadable: false,
        }),
    );
    if had_one {
        inner.forget(&host, &previous).await;
    }
    Ok(inner.listed(saved))
}
