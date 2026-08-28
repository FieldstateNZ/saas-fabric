//! The port an identity provider implements.

mod errors;
mod observed;

use async_trait::async_trait;
use fabric_client_model::{OidcClient, RealmName, RoleName};

pub use errors::ProviderError;
pub use observed::{ObservedOidcClient, ObservedRealm};

/// What SaaS Fabric needs an identity provider to be able to do.
///
/// # Written in the platform's vocabulary, on purpose
///
/// Every parameter here is a type from `fabric-client-model`. Nothing in this
/// trait is shaped by any particular provider's API — no representation
/// structs, no internal object ids, no protocol version. That is the whole
/// point of the seam: an adapter translates, and the translation stops there
/// (ADR 0008).
///
/// It is deliberately *not* an abstraction over "any identity provider" for
/// its own sake. The operations are the ones reconciliation actually performs,
/// no more, and a second implementation would be judged by whether it can
/// honour them — not by whether this trait avoided saying anything specific.
///
/// # Every operation must be idempotent
///
/// Creating a realm, a role, or an application client that already exists must
/// **succeed**, not fail. The reconciler diffs first and does not ask for work
/// it can see is unnecessary, but the two are separated by a network and by
/// whatever the provider does on its own — Keycloak creates several roles with
/// every realm — so an adapter that returned an error for "already exists"
/// would make reconciliation flap for reasons no operator could see.
///
/// # Nothing here deletes
///
/// There is no `delete_realm_role`, and its absence is a decision rather than
/// an omission. See this crate's documentation.
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Reads a realm's current state, or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the provider could not be reached or
    /// refused the read. A realm that is absent is `Ok(None)`, not an error —
    /// the difference is what the reconciler branches on.
    async fn observe_realm(&self, realm: &RealmName) -> Result<Option<ObservedRealm>, ProviderError>;

    /// Creates a realm, or does nothing if it already exists.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the realm could not be created.
    async fn create_realm(&self, realm: &RealmName, display_name: &str) -> Result<(), ProviderError>;

    /// Sets a realm's display name.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the realm could not be updated.
    async fn set_realm_display_name(
        &self,
        realm: &RealmName,
        display_name: &str,
    ) -> Result<(), ProviderError>;

    /// Creates a realm role, or does nothing if it already exists.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the role could not be created.
    async fn create_realm_role(&self, realm: &RealmName, role: &RoleName) -> Result<(), ProviderError>;

    /// Creates an application client, or does nothing if it already exists.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the client could not be created.
    async fn create_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError>;

    /// Brings an existing application client in line with the declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the client could not be updated.
    async fn update_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError>;

    /// A short description for logging, such as an endpoint.
    ///
    /// Must not contain a credential. Reconciliation logs this on every pass.
    fn describe(&self) -> String;
}
