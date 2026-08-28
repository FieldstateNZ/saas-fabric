//! Keycloak's client representation.

/// An application client as Keycloak reports it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ClientRepresentation {
    /// Keycloak's internal identifier, which is what an update is addressed
    /// to.
    ///
    /// Distinct from `clientId`, and the distinction matters: the path for an
    /// update is `/clients/{id}` using *this* value, while everything a human
    /// or a document says uses `clientId`. Sending one where the other belongs
    /// produces a 404 that reads like the client does not exist.
    pub(crate) id: String,

    /// The identifier an application presents when authenticating.
    #[serde(rename = "clientId")]
    pub(crate) client_id: String,

    /// The redirect URIs currently registered.
    #[serde(rename = "redirectUris", default)]
    pub(crate) redirect_uris: Vec<String>,

    /// Whether Keycloak holds it as a public client.
    #[serde(rename = "publicClient", default)]
    pub(crate) public_client: bool,
}

/// An application client as SaaS Fabric declares it.
///
/// Used for both create and update. The field set is the whole of what the
/// client document can express, which is what makes an update idempotent: a
/// second write of the same declaration produces the same object.
#[derive(Debug, serde::Serialize)]
pub(crate) struct NewClientRepresentation<'a> {
    /// The identifier an application presents.
    #[serde(rename = "clientId")]
    pub(crate) client_id: &'a str,

    /// Whether the client may be used at all.
    pub(crate) enabled: bool,

    /// Always `openid-connect`.
    ///
    /// Sent explicitly rather than left to Keycloak's default, because the
    /// document says `type: oidc` and a default is not a statement.
    pub(crate) protocol: &'static str,

    /// Always `true` — every client SaaS Fabric declares is public.
    ///
    /// A confidential client would need a secret, and secrets never enter
    /// desired state. See `OidcClient` in `fabric-client-model`.
    #[serde(rename = "publicClient")]
    pub(crate) public_client: bool,

    /// Enables the authorisation-code flow, which is what a browser client
    /// needs and the only flow these redirect URIs are meaningful for.
    #[serde(rename = "standardFlowEnabled")]
    pub(crate) standard_flow_enabled: bool,

    /// Where Keycloak may redirect back to.
    #[serde(rename = "redirectUris")]
    pub(crate) redirect_uris: Vec<String>,
}
