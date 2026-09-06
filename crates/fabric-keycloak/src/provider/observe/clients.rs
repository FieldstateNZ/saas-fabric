//! Reading a realm's application clients.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{OidcClientId, PkceMethod, RealmName, RedirectUri};
use fabric_reconciliation::{ObservedOidcClient, ProviderError};

use crate::admin::KeycloakAdmin;
use crate::wire::{
    ClientRepresentation, ProtocolMapperRepresentation, AUDIENCE_MAPPER_CONFIG_KEY, AUDIENCE_MAPPER_TYPE,
    PKCE_CHALLENGE_METHOD_ATTRIBUTE,
};

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

            Some((
                id,
                ObservedOidcClient {
                    redirect_uris,
                    public: client.public_client,
                    challenge_method: challenge_method(&client.attributes),
                    audience_mapper: audience_mapper(&client.protocol_mappers),
                    unmodellable_redirect_uris,
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
/// log line, or an API response.
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

/// The configured audience of a client's first `oidc-audience-mapper`, if it
/// has one. "First": this adapter never writes more than one, but nothing
/// stops an operator adding a second by hand — that is drift for the
/// reconciler to notice some other way, not something this read enumerates.
fn audience_mapper(mappers: &[ProtocolMapperRepresentation]) -> Option<String> {
    mappers
        .iter()
        .find(|mapper| mapper.protocol_mapper == AUDIENCE_MAPPER_TYPE)
        .and_then(|mapper| mapper.config.get(AUDIENCE_MAPPER_CONFIG_KEY))
        .cloned()
}
