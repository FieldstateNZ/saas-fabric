//! Which identity provider this deployment reconciles.

use fabric_keycloak::KeycloakConfig;

/// The identity provider reconciliation converges.
///
/// Tagged for the same reason [`DesiredStateConfig`](super::DesiredStateConfig)
/// is: the in-memory provider reports every client as converged, so a
/// deployment that reached it by omission would show a screen full of green
/// ticks over an identity provider nothing had ever written to.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum IdentityProviderConfig {
    /// Keycloak, over its admin REST API.
    Keycloak(KeycloakConfig),

    /// An identity provider held in memory.
    ///
    /// **Development only.** It honours the port's semantics — creates are
    /// idempotent, observation reflects what was written — so reconciliation
    /// behaves exactly as it would against Keycloak, and a second pass really
    /// does change nothing (§22). What it does not do is outlive the process.
    InMemory,
}
