//! Application ids every realm already has a client under, whether or not a
//! catalogue ever declares one.

/// OIDC client ids Keycloak creates in every realm it manages, before this
/// platform ever writes an application into it.
///
/// An application id becomes an OIDC client id in the realm it is assigned to
/// (`with_application_identity`), and Keycloak does not refuse a second
/// client sharing one of these ids — it would sit beside the realm-managed
/// one, silently indistinguishable from it in the admin console and in this
/// platform's own reconciliation. Declaring an application under one of these
/// names is refused where the id is chosen, not left to surface later as a
/// confusing identity conflict.
///
/// Not configuration: unlike the console's own client id or a converged
/// Keycloak's admin client id (see
/// `fabric-control-plane-api::startup::reserved_names`, which composes
/// *those* from deployment config and passes them in as plain strings), these
/// six are Keycloak's own built-ins in every realm it creates, named the same
/// way in every deployment — nothing for a composition root to compute.
pub const RESERVED_APPLICATION_IDS: [&str; 6] = [
    "account",
    "account-console",
    "admin-cli",
    "broker",
    "realm-management",
    "security-admin-console",
];

#[cfg(test)]
mod tests {
    use super::RESERVED_APPLICATION_IDS;
    use crate::OidcClientId;

    #[test]
    fn every_reserved_id_is_itself_a_legal_oidc_client_id() {
        for id in RESERVED_APPLICATION_IDS {
            assert!(OidcClientId::try_new(id).is_ok(), "{id}");
        }
    }
}
