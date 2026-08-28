//! The identifier of an application client within a realm.

slug_newtype!(
    /// The `client_id` an application presents when authenticating, such as
    /// `web`.
    ///
    /// Written by a platform engineer in the client document rather than
    /// derived from anything a tenant supplies, so it takes the more permissive
    /// identifier rule — `mobile_app` is a reasonable client id and would fail
    /// the DNS rule.
    ///
    /// Not to be confused with [`ClientId`](super::ClientId): that names the
    /// organisation, this names one application belonging to it. The
    /// collision of the word "client" is Keycloak's and OAuth's, not this
    /// platform's, and it is the reason the two types are named apart.
    OidcClientId,
    "oidc client id",
    fabric_core::naming::parse_identifier
);
