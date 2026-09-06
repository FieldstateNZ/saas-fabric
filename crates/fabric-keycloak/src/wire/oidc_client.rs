//! Keycloak's client representation.
//!
//! In the 121–150 line band: a cohesive wire-format type per
//! `docs/architecture/file-size-policy.md` — the read shape, the write shape
//! and their one shared mapper type, plus the four vocabulary strings this
//! crate exists to contain. Splitting read from write would separate one
//! protocol's two directions, not two concepts.

use std::collections::BTreeMap;

/// Attribute key: the PKCE challenge method Keycloak enforces at its
/// authorization endpoint (RFC 7636 §4.3). The *value* (`S256`) is not this
/// crate's vocabulary — it is `PkceMethod::as_wire_value()` in
/// `fabric-client-model`, so the write and the compare cannot disagree.
pub(crate) const PKCE_CHALLENGE_METHOD_ATTRIBUTE: &str = "pkce.code.challenge.method";

/// Attribute key: where Keycloak may redirect back to after logout. Always
/// written as the literal value `+`, Keycloak's shorthand for "this client's
/// registered redirect URIs" — one list, so a second cannot drift from it.
pub(crate) const POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE: &str = "post.logout.redirect.uris";

/// Keycloak's built-in protocol mapper type that adds a fixed string to a
/// token's `aud` claim.
pub(crate) const AUDIENCE_MAPPER_TYPE: &str = "oidc-audience-mapper";

/// The `oidc-audience-mapper`'s config key carrying the audience string.
pub(crate) const AUDIENCE_MAPPER_CONFIG_KEY: &str = "included.custom.audience";

/// An application client as Keycloak reports it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ClientRepresentation {
    /// Keycloak's internal identifier, which is what an update is addressed
    /// to. Distinct from `clientId`: the update path uses this value, while a
    /// human or a document uses `clientId`. Sending one where the other
    /// belongs produces a 404 that reads like the client does not exist.
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

    /// Non-standard settings, including the PKCE challenge method and the
    /// post-logout redirect set. `#[serde(default)]`: a client with no
    /// `attributes` key at all — the shape before this slice, or one edited by
    /// hand — reads as an empty map rather than failing the whole response.
    /// Absence is drift for the reconciler to decide about, not a parse error.
    #[serde(default)]
    pub(crate) attributes: BTreeMap<String, String>,

    /// The client's protocol mappers, including the audience mapper if one
    /// exists.
    #[serde(rename = "protocolMappers", default)]
    pub(crate) protocol_mappers: Vec<ProtocolMapperRepresentation>,
}

/// One protocol mapper, as Keycloak reports it. Only the two fields
/// observation needs — which mapper type it is, and its config — everything
/// else Keycloak returns (`id`, `consentRequired`, …) is never read.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProtocolMapperRepresentation {
    /// Which kind of mapper this is, e.g. `oidc-audience-mapper`.
    #[serde(rename = "protocolMapper")]
    pub(crate) protocol_mapper: String,

    /// The mapper's settings.
    #[serde(default)]
    pub(crate) config: BTreeMap<String, String>,
}

/// An application client as SaaS Fabric declares it.
///
/// Used for both create and update. The field set is the whole of what the
/// client document can express, which is what makes an update idempotent: a
/// second write of the same declaration produces the same object — and,
/// because Keycloak replaces a client's mapper set by name on every `PUT`
/// rather than merging it, the same is true of `protocolMappers`.
#[derive(Debug, serde::Serialize)]
pub(crate) struct NewClientRepresentation<'a> {
    /// The identifier an application presents.
    #[serde(rename = "clientId")]
    pub(crate) client_id: &'a str,

    /// Whether the client may be used at all.
    pub(crate) enabled: bool,

    /// Always `openid-connect`. Sent explicitly rather than left to
    /// Keycloak's default, because the document says `type: oidc` and a
    /// default is not a statement.
    pub(crate) protocol: &'static str,

    /// Always `true` — every client SaaS Fabric declares is public. A
    /// confidential client would need a secret, and secrets never enter
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

    /// The PKCE challenge method and the post-logout redirect set.
    /// `webOrigins` is deliberately not part of this map, or of this type at
    /// all — ADR 0019 leaves CORS undecided.
    pub(crate) attributes: BTreeMap<&'static str, String>,

    /// The client's protocol mappers. Exactly one today: the audience mapper.
    #[serde(rename = "protocolMappers")]
    pub(crate) protocol_mappers: Vec<AudienceMapper<'a>>,
}

/// The audience mapper every declared client carries.
///
/// Keycloak's `oidc-audience-mapper` adds a fixed string to a token's `aud`
/// claim. The edge's audience check (ADR 0019 §G5) refuses every genuine token
/// until this mapper exists on the client that issued it.
///
/// `config` is a plain map, not a named type: of its three keys, only
/// `included.custom.audience` (see [`AUDIENCE_MAPPER_CONFIG_KEY`]) is
/// vocabulary this crate owns — `access.token.claim` and `id.token.claim` are
/// Keycloak's generic "which token carries this claim" switches, shared by
/// every mapper type.
#[derive(Debug, serde::Serialize)]
pub(crate) struct AudienceMapper<'a> {
    /// A fixed, human-readable name. Keycloak requires one; nothing reads it
    /// back.
    pub(crate) name: &'static str,

    /// Always `openid-connect`, matching the client's own protocol.
    pub(crate) protocol: &'static str,

    /// Keycloak's built-in mapper type that adds a custom audience string.
    #[serde(rename = "protocolMapper")]
    pub(crate) protocol_mapper: &'static str,

    /// The mapper's settings — see the type-level note on its keys.
    pub(crate) config: BTreeMap<&'static str, &'a str>,
}
