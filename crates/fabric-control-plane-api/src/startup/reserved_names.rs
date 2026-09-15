//! Names an operator must never be able to give a client — a realm, or an
//! application id — computed once here from whatever this deployment
//! configures.
//!
//! `fabric-control-plane` accepts a plain set of names for each and knows
//! nothing about where they came from (see
//! [`ClientService`](fabric_control_plane::ClientService)'s own rustdoc):
//! this is the one place in the whole platform that is allowed to know both
//! what a client is and what Keycloak's own realms and clients are called,
//! because composing that knowledge is exactly what a composition root is
//! for.

use std::collections::BTreeSet;

use fabric_client_model::RealmName;
use fabric_control_plane::{ControlPlaneConfig, OperatorConfig};
use fabric_keycloak::KeycloakConfig;

use crate::config::IdentityProviderConfig;

/// Realms a client document may never declare.
///
/// `master` always — Keycloak's own realm, present whether or not this
/// deployment's machine identity happens to live in it. The operator
/// posture's own realm, parsed from its issuer URL, because a client
/// declaring the realm operators authenticate against would let the next
/// reconciliation pass rewrite it with that operator's own bearer. And, when
/// this deployment converges Keycloak, the realm its machine identity lives
/// in — `admin_realm`, `master` by that config's own default but not
/// guaranteed to stay `master` in every deployment.
///
/// # Errors
///
/// Returns a message if the operator posture's issuer names no realm — an
/// issuer this deployment already needs to build the identity posture at
/// all, so a value that fails this parse would already have failed to sign
/// anybody in.
pub(super) fn realms(
    control_plane: &ControlPlaneConfig,
    identity_provider: &IdentityProviderConfig,
) -> Result<BTreeSet<RealmName>, String> {
    let mut realms = BTreeSet::new();

    realms.insert(RealmName::try_new("master").map_err(|error| error.to_string())?);
    realms.insert(operator_realm(&control_plane.operator)?);

    if let IdentityProviderConfig::Keycloak(keycloak) = identity_provider {
        realms.insert(RealmName::try_new(&keycloak.admin_realm).map_err(|error| error.to_string())?);
    }

    Ok(realms)
}

/// The realm segment of the operator posture's own issuer URL.
///
/// A Keycloak issuer is always `<base>/realms/<name>`, so the last non-empty
/// path segment is the realm — parsed positionally rather than by matching
/// literal `/realms/`, because the base URL itself could contain that
/// substring.
fn operator_realm(operator: &OperatorConfig) -> Result<RealmName, String> {
    let OperatorConfig::Oidc { issuer, .. } = operator;

    let realm = issuer
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .ok_or_else(|| format!("the operator issuer '{issuer}' names no realm"))?;

    RealmName::try_new(realm).map_err(|error| error.to_string())
}

/// Application ids the catalogue may never accept, beyond the
/// realm-managed built-ins `fabric-client-model` already refuses on its own.
///
/// The console's own OIDC client id — it is not an application, and
/// Keycloak would happily create a second client under the same id, leaving
/// the console indistinguishable from an assigned application in its own
/// realm — and, when this deployment converges Keycloak, its machine
/// identity's own `client_id`.
///
/// Plain strings, not [`OidcClientId`](fabric_client_model::OidcClientId):
/// what matters here is an exact match against an application id someone
/// typed, which is a string comparison regardless of either side's shape.
pub(super) fn client_ids(
    control_plane: &ControlPlaneConfig,
    identity_provider: &IdentityProviderConfig,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();

    let OperatorConfig::Oidc { client_id, .. } = &control_plane.operator;
    ids.insert(client_id.clone());

    if let IdentityProviderConfig::Keycloak(KeycloakConfig { client_id, .. }) = identity_provider {
        ids.insert(client_id.clone());
    }

    ids
}
