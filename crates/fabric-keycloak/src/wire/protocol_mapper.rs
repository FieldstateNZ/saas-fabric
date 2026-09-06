//! Keycloak's protocol mapper: a read shape and a write shape for one concept.
//!
//! A protocol mapper is one Keycloak object, but this crate never round-trips
//! it through a single type — the same reason `oidc_client.rs` keeps
//! `ClientRepresentation` and `NewClientRepresentation` apart.
//! [`ProtocolMapperRepresentation`] is what `GET /clients` hands back, and it
//! reads only the two fields observation needs (which mapper type, and its
//! config); Keycloak returns several more (`id`, `consentRequired`, …) that
//! this crate has no reason to model. [`AudienceMapper`] is the shape this
//! adapter constructs to send: it carries a fixed `name` Keycloak requires on
//! write and never returns on read, so a type built to deserialise a mapper
//! could not also serialise one. Splitting the struct from its sibling would
//! separate a request from its matching response; that is not this file's
//! situation, so it keeps both together instead.
//!
//! # Why `included.custom.audience`, not `included.client.audience`
//!
//! Keycloak's `oidc-audience-mapper` takes its audience string from one of two
//! mutually exclusive config keys: `included.client.audience` names an
//! *existing registered client*, whose `clientId` becomes the audience, and
//! `included.custom.audience` is an arbitrary operator-supplied string. The
//! Data API's audience (ADR 0019 §1/§G5 — e.g. `saas-fabric-data-api`) names a
//! resource server this platform never registers as a Keycloak client, so the
//! client-scoped key cannot express it; only the custom key can.

use std::collections::BTreeMap;

/// Keycloak's built-in protocol mapper type that adds a fixed string to a
/// token's `aud` claim.
pub(crate) const AUDIENCE_MAPPER_TYPE: &str = "oidc-audience-mapper";

/// The `oidc-audience-mapper`'s config key carrying the audience string.
pub(crate) const AUDIENCE_MAPPER_CONFIG_KEY: &str = "included.custom.audience";

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
