//! What the fake does when reconciliation calls it.
//!
//! Split from the type's own file because these are two genuinely different
//! concerns that happen to share a struct: the other file is the surface a
//! *test* drives — seed a realm, inject a failure, read back the calls — and
//! this one is the surface *reconciliation* drives. The house convention for
//! that split is an impl block in its own module (see `config::loading` in the
//! runtime host), not one long file.
//!
//! # Why this file is over 120 lines
//!
//! It is one `impl IdentityProvider` block — every method the port declares,
//! answered the way an in-memory realm answers it — plus the one private
//! helper (`write_client`) two of those methods share. Splitting the eight
//! methods of a single trait impl across files would separate things that are
//! one concept precisely because they are one impl: the whole point of this
//! file is "everything reconciliation can ask of the fake," in one place a
//! reader can check against the trait's own declaration.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use fabric_client_model::{OidcClient, RealmName, RoleName};

use crate::provider::{IdentityProvider, ObservedOidcClient, ObservedRealm, ProviderError};
use crate::testing::fake_identity_provider::{lock, FakeIdentityProvider};

/// The roles a real identity provider creates for itself with every realm.
///
/// Seeded so that the "an already-correct realm produces no change" property is
/// tested against a realm that holds roles the document never mentioned —
/// which is the normal case, and the one a reconciler that compared sets for
/// equality would get wrong.
const PROVIDER_OWNED_ROLES: [&str; 2] = ["offline_access", "uma_authorization"];

/// The audience this fake's mapper asserts on every client it writes.
///
/// A real provider's audience is its own deployment configuration (ADR 0019
/// §G5); this fake has no configuration to read one from, so it asserts a
/// fixed value instead. One constant is enough for what this crate's tests
/// need — a client written through the fake to come back converged — and
/// giving the fake a configurable audience would be surface for a difference
/// no test here exercises.
const AUDIENCE: &str = "saas-fabric-data-api";

#[async_trait]
impl IdentityProvider for FakeIdentityProvider {
    async fn observe_realm(&self, realm: &RealmName) -> Result<Option<ObservedRealm>, ProviderError> {
        self.enter(format!("observe_realm:{realm}"))?;

        Ok(lock(&self.realms).get(realm).cloned())
    }

    async fn create_realm(&self, realm: &RealmName, display_name: &str) -> Result<(), ProviderError> {
        self.enter(format!("create_realm:{realm}"))?;

        let mut roles = BTreeSet::new();
        for name in PROVIDER_OWNED_ROLES {
            if let Ok(role) = RoleName::try_new(name) {
                roles.insert(role);
            }
        }

        // Idempotent, as the port requires: an existing realm is left as it is.
        lock(&self.realms).entry(realm.clone()).or_insert(ObservedRealm {
            display_name: display_name.to_owned(),
            roles,
            clients: BTreeMap::new(),
        });

        Ok(())
    }

    async fn set_realm_display_name(
        &self,
        realm: &RealmName,
        display_name: &str,
    ) -> Result<(), ProviderError> {
        self.enter(format!("set_realm_display_name:{realm}"))?;

        if let Some(existing) = lock(&self.realms).get_mut(realm) {
            display_name.clone_into(&mut existing.display_name);
        }

        Ok(())
    }

    async fn create_realm_role(&self, realm: &RealmName, role: &RoleName) -> Result<(), ProviderError> {
        self.enter(format!("create_realm_role:{realm}:{role}"))?;

        if let Some(existing) = lock(&self.realms).get_mut(realm) {
            existing.roles.insert(role.clone());
        }

        Ok(())
    }

    async fn create_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError> {
        self.enter(format!("create_oidc_client:{realm}:{}", client.id))?;
        self.write_client(realm, client);

        Ok(())
    }

    async fn update_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError> {
        self.enter(format!("update_oidc_client:{realm}:{}", client.id))?;
        self.write_client(realm, client);

        Ok(())
    }

    fn configured_audience(&self) -> &'static str {
        AUDIENCE
    }

    fn describe(&self) -> String {
        "in-memory identity provider".to_owned()
    }
}

impl FakeIdentityProvider {
    /// Stores an application client exactly as it was declared.
    ///
    /// `audience_mapper` carries this fake's own [`AUDIENCE`] — the same
    /// value [`IdentityProvider::configured_audience`] reports — mirroring a
    /// real provider's write-then-read-back. Leaving it `None` here would
    /// make every client written through this fake permanently drifted the
    /// moment `matches()` started comparing it.
    fn write_client(&self, realm: &RealmName, client: &OidcClient) {
        if let Some(existing) = lock(&self.realms).get_mut(realm) {
            existing.clients.insert(
                client.id.clone(),
                ObservedOidcClient {
                    redirect_uris: client.redirect.uris().iter().cloned().collect(),
                    public: true,
                    challenge_method: Some(client.pkce),
                    audience_mapper: Some(AUDIENCE.to_owned()),
                    unmodellable_redirect_uris: 0,
                },
            );
        }
    }
}
