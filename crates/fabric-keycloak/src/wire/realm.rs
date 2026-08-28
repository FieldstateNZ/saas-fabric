//! Keycloak's realm representation, reduced to what is reconciled.

/// A realm as Keycloak reports it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct RealmRepresentation {
    /// The realm's display name, which is what SaaS Fabric reconciles.
    ///
    /// Optional in Keycloak — a realm created without one has no
    /// `displayName` at all — so an absent value is read as an empty name
    /// rather than as a failure to parse the response.
    #[serde(rename = "displayName", default)]
    pub(crate) display_name: Option<String>,
}

/// A realm as SaaS Fabric creates it.
#[derive(Debug, serde::Serialize)]
pub(crate) struct NewRealmRepresentation<'a> {
    /// The realm name, which is also its identifier.
    pub(crate) realm: &'a str,

    /// The name an operator sees in Keycloak.
    #[serde(rename = "displayName")]
    pub(crate) display_name: &'a str,

    /// Whether the realm accepts logins.
    ///
    /// Always `true`. A disabled realm is not a state SaaS Fabric can express,
    /// so creating one disabled would produce a client that reconciles
    /// successfully and cannot sign anybody in.
    pub(crate) enabled: bool,
}

/// The only realm field SaaS Fabric ever updates.
///
/// Deliberately two fields rather than a full representation. Keycloak's realm
/// update applies the fields it is given, so a fuller body would reset
/// everything an operator had configured directly — token lifespans, password
/// policy, themes — every time a display name changed.
#[derive(Debug, serde::Serialize)]
pub(crate) struct RealmUpdate<'a> {
    /// The realm being updated. Keycloak requires it in the body as well as
    /// the path.
    pub(crate) realm: &'a str,

    /// The name to set.
    #[serde(rename = "displayName")]
    pub(crate) display_name: &'a str,
}
