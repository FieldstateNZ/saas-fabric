//! Reading a realm's application clients.
//!
//! This is one read (`read`) and the three small, pure decompositions it
//! exists to keep readable — `partition_uris`, `challenge_method`,
//! `is_every_registered_uri` — each turning one field of Keycloak's wire
//! shape into the one thing `ObservedOidcClient` needs from it. None of the
//! four is reused, tested, or meaningful outside this read, so splitting them
//! into their own files would scatter one concept across four, not separate
//! two. What a client's protocol-mapper list says is a fifth such fact, but a
//! two-part one computed from one pass over its own short list — see
//! `protocol_mappers`, split out rather than added here.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{OidcClientId, PkceMethod, RealmName, RedirectUri};
use fabric_reconciliation::{ObservedOidcClient, ProviderError};

use crate::admin::KeycloakAdmin;
use crate::wire::{
    ClientRepresentation, PKCE_CHALLENGE_METHOD_ATTRIBUTE, POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE,
};

mod protocol_mappers;

/// The most application clients this adapter will read in one request.
///
/// Same reasoning as roles' page bound: a truncated list could also hide the
/// one client carrying an unmodellable redirect URI (ADR 0019 §6).
const CLIENT_PAGE: usize = 2000;

/// Reads a realm's application clients.
pub(super) async fn read(
    admin: &KeycloakAdmin,
    realm: &RealmName,
) -> Result<BTreeMap<OidcClientId, ObservedOidcClient>, ProviderError> {
    let representations: Vec<ClientRepresentation> = admin
        .get(
            "reading application clients",
            admin.paths().clients_page(realm, CLIENT_PAGE),
        )
        .await?;

    if representations.len() >= CLIENT_PAGE {
        return Err(ProviderError::Rejected {
            detail: format!("the realm has more than {CLIENT_PAGE} clients, which this adapter cannot read"),
        });
    }

    Ok(representations
        .into_iter()
        .filter_map(|client| {
            let id = OidcClientId::try_new(&client.client_id).ok()?;
            let (redirect_uris, unmodellable_redirect_uris) = partition_uris(&client.redirect_uris);
            let (audience_mapper, other_protocol_mappers) = protocol_mappers::read(&client.protocol_mappers);

            Some((
                id,
                ObservedOidcClient {
                    redirect_uris,
                    public: client.public_client,
                    challenge_method: challenge_method(&client.attributes),
                    audience_mapper,
                    other_protocol_mappers,
                    unmodellable_redirect_uris,
                    enabled: client.enabled,
                    standard_flow_enabled: client.standard_flow_enabled,
                    post_logout_redirect_uris_is_every_registered_uri: is_every_registered_uri(
                        &client.attributes,
                    ),
                },
            ))
        })
        .collect())
}

/// Splits a client's raw redirect URIs into what this model can parse, and
/// how many it cannot.
///
/// An unparseable redirect URI is drift, not silence (ADR 0019 §6): the count
/// travels so reconciliation can rewrite the client, but the value never
/// does — it is attacker-influenced text with no reason to reach a plan, a
/// log line, or an API response. The parsed side is a *set*, so two raw
/// entries that parse to the same [`RedirectUri`] collapse into one member —
/// the unmodellable count is not `raw.len() - parsed.len()`, it is exactly
/// the number of entries that failed to parse.
fn partition_uris(raw: &[String]) -> (BTreeSet<RedirectUri>, usize) {
    let mut parsed = BTreeSet::new();
    let mut unmodellable = 0_usize;

    for uri in raw {
        match RedirectUri::try_new(uri) {
            Ok(uri) => {
                parsed.insert(uri);
            }
            Err(_) => unmodellable += 1,
        }
    }

    (parsed, unmodellable)
}

/// Reads the PKCE challenge method Keycloak holds, as this model understands
/// it.
///
/// `None` covers both "absent" and "holds a value this model does not
/// recognise" (`plain`, empty, a typo) — both are drift from `Some(S256)`, so
/// no `Plain` variant is needed anywhere in the model to notice a downgrade.
fn challenge_method(attributes: &BTreeMap<String, String>) -> Option<PkceMethod> {
    let value = attributes.get(PKCE_CHALLENGE_METHOD_ATTRIBUTE)?;

    (value == PkceMethod::S256.as_wire_value()).then_some(PkceMethod::S256)
}

/// Whether the post-logout redirect attribute still holds the literal `+`
/// a declaration always writes — Keycloak's own shorthand for "every
/// registered redirect URI," which this model does not otherwise parse.
fn is_every_registered_uri(attributes: &BTreeMap<String, String>) -> bool {
    attributes
        .get(POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE)
        .map(String::as_str)
        == Some("+")
}
