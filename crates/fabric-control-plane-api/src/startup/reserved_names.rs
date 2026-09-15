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
/// # Why these are plain, case-folded strings and never a [`RealmName`](fabric_client_model::RealmName)
///
/// A client-declared realm is always DNS-label-shaped, because
/// [`ClientId`](fabric_client_model::ClientId) is — but an operator issuer's
/// realm segment and a configured `admin_realm` are Keycloak realm names
/// Keycloak itself never constrains that way: mixed case, underscores and
/// periods are all realms Keycloak accepts. Parsing either through
/// [`RealmName::try_new`](fabric_client_model::RealmName::try_new) once
/// meant a deployment whose operator issuer or `admin_realm` was, say,
/// `Fabric` or `saas_fabric` — both realm names Keycloak had already
/// accepted, and both realms this deployment may have been running against
/// for months — refused to start the moment this reserved-names check was
/// added. Comparing case-folded strings instead means this check can never
/// be the reason a previously-working deployment stops starting.
///
/// # Errors
///
/// Returns a message if the operator posture's issuer names no realm at
/// all — an issuer this deployment already needs to build the identity
/// posture, so a value that fails this would already have failed to sign
/// anybody in.
pub(super) fn realms(
    control_plane: &ControlPlaneConfig,
    identity_provider: &IdentityProviderConfig,
) -> Result<BTreeSet<String>, String> {
    let mut realms = BTreeSet::new();

    realms.insert("master".to_owned());
    realms.insert(operator_realm(&control_plane.operator)?);

    if let IdentityProviderConfig::Keycloak(keycloak) = identity_provider {
        realms.insert(keycloak.admin_realm.to_lowercase());
    }

    Ok(realms)
}

/// The realm segment of the operator posture's own issuer URL, case-folded.
///
/// A Keycloak issuer is always `<base>/realms/<name>`, so the last non-empty
/// path segment is the realm — parsed positionally rather than by matching
/// literal `/realms/`, because the base URL itself could contain that
/// substring. Not validated as a [`RealmName`](fabric_client_model::RealmName)
/// — see [`realms`]'s own rustdoc for why that parse must not run here.
fn operator_realm(operator: &OperatorConfig) -> Result<String, String> {
    let OperatorConfig::Oidc { issuer, .. } = operator;

    issuer
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(str::to_lowercase)
        .ok_or_else(|| format!("the operator issuer '{issuer}' names no realm"))
}

/// Application ids the catalogue may never accept, beyond the
/// realm-managed built-ins `fabric-client-model` already refuses on its own.
///
/// The console's own OIDC client id — it is not an application — and, when
/// this deployment converges Keycloak, its machine identity's own
/// `client_id`. See
/// [`ClientService::check_application_id_available`](fabric_control_plane::ClientService)'s
/// own rustdoc for what letting either through would actually cost: not
/// Keycloak refusing a duplicate id (it does — realm `clientId`s are
/// unique), but this platform's own idempotent-create convention silently
/// turning that refusal into a later overwrite.
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

#[cfg(test)]
mod tests {
    use super::{operator_realm, realms};
    use crate::config::IdentityProviderConfig;
    use fabric_control_plane::{ControlPlaneConfig, OperatorConfig, ReconciliationConfig};
    use fabric_keycloak::KeycloakConfig;

    fn operator(issuer: &str) -> OperatorConfig {
        OperatorConfig::Oidc {
            issuer: issuer.to_owned(),
            reachable_at: String::new(),
            client_id: "console".to_owned(),
            required_role: "fabric-operator".to_owned(),
            redirect_uri: "https://console.example.test/".to_owned(),
            leeway_seconds: 60,
            jwks_refresh_seconds: 300,
        }
    }

    fn control_plane(issuer: &str) -> ControlPlaneConfig {
        ControlPlaneConfig {
            public_base_url: String::new(),
            operator: operator(issuer),
            reconciliation: ReconciliationConfig::default(),
        }
    }

    fn keycloak(admin_realm: &str) -> KeycloakConfig {
        KeycloakConfig {
            base_url: "https://keycloak.example.test".to_owned(),
            admin_realm: admin_realm.to_owned(),
            client_id: "svc-fabric".to_owned(),
            http_timeout_seconds: 5,
            audience: "fabric-data-api".to_owned(),
        }
    }

    #[test]
    fn an_issuer_ending_in_realms_and_a_name_names_that_realm() {
        assert_eq!(
            operator_realm(&operator("https://auth.example.test/realms/fabric")).unwrap(),
            "fabric"
        );
    }

    // `operator_realm` is a purely positional parse — the last non-empty
    // `/`-separated segment — so an issuer with no `/realms/<name>` path
    // does not error; it names whatever segment happens to be last. Only a
    // value with no non-empty segment at all (nothing this deployment's own
    // issuer could ever be, since it must already be a URL the identity
    // posture can be built from) reaches the error branch.
    #[test]
    fn an_issuer_with_no_realms_path_still_names_its_last_segment() {
        assert_eq!(
            operator_realm(&operator("https://auth.example.test")).unwrap(),
            "auth.example.test"
        );
    }

    #[test]
    fn an_issuer_with_no_segment_at_all_names_no_realm() {
        assert!(operator_realm(&operator("")).is_err());
    }

    #[test]
    fn a_mixed_case_realm_segment_is_folded_to_lowercase_rather_than_refused() {
        assert_eq!(
            operator_realm(&operator("https://auth.example.test/realms/Fabric")).unwrap(),
            "fabric"
        );
    }

    #[test]
    fn an_underscore_realm_segment_is_kept_rather_than_refused() {
        assert_eq!(
            operator_realm(&operator("https://auth.example.test/realms/saas_fabric")).unwrap(),
            "saas_fabric"
        );
    }

    #[test]
    fn a_mixed_case_admin_realm_does_not_fail_startup() {
        let reserved = realms(
            &control_plane("https://auth.example.test/realms/master"),
            &IdentityProviderConfig::Keycloak(keycloak("Fabric")),
        )
        .unwrap();

        assert!(reserved.contains("fabric"), "{reserved:?}");
    }

    #[test]
    fn an_underscore_admin_realm_does_not_fail_startup() {
        let reserved = realms(
            &control_plane("https://auth.example.test/realms/master"),
            &IdentityProviderConfig::Keycloak(keycloak("saas_fabric")),
        )
        .unwrap();

        assert!(reserved.contains("saas_fabric"), "{reserved:?}");
    }

    #[test]
    fn without_keycloak_configured_only_master_and_the_operator_realm_are_reserved() {
        let reserved = realms(
            &control_plane("https://auth.example.test/realms/acme"),
            &IdentityProviderConfig::InMemory,
        )
        .unwrap();

        assert_eq!(
            reserved,
            std::collections::BTreeSet::from(["master".to_owned(), "acme".to_owned()])
        );
    }
}
