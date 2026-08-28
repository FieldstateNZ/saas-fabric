//! Keycloak's realm-role representation.

/// A realm role as Keycloak reports it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct RoleRepresentation {
    /// The role's name, which is what SaaS Fabric matches on.
    pub(crate) name: String,
}

/// A realm role as SaaS Fabric creates it.
///
/// Name only. Keycloak fills in the rest, and sending a description or a
/// composite flag would mean SaaS Fabric asserting things about a role it has
/// no opinion about.
#[derive(Debug, serde::Serialize)]
pub(crate) struct NewRoleRepresentation<'a> {
    /// The role to create.
    pub(crate) name: &'a str,
}
