//! The name of a client's identity realm.

slug_newtype!(
    /// The realm a client's identity lives in.
    ///
    /// A SaaS Fabric concept that Keycloak happens to implement as a realm
    /// (see ADR 0008). Nothing above the Keycloak adapter may assume more
    /// about it than this type says.
    ///
    /// Strict DNS rule, because the value is interpolated into Keycloak admin
    /// API paths (`/admin/realms/{realm}`) — a realm name containing a slash
    /// or a `..` would address a different resource entirely, and parsing is
    /// where that is closed off rather than at each call site.
    RealmName,
    "realm name",
    fabric_core::naming::parse_dns_label
);
