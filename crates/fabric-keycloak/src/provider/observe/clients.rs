//! Reading a realm's application clients.
//!
//! In the 121–150 line band: this is one read (`read`) and the three small,
//! pure decompositions it exists to keep readable — `partition_uris`,
//! `challenge_method`, `audience_mapper` — each turning one field of
//! Keycloak's wire shape into the one thing `ObservedOidcClient` needs from
//! it. None of the four is reused, tested, or meaningful outside this read,
//! so splitting them into their own files would scatter one concept across
//! four, not separate two.

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

/// The configured audience of a client's `oidc-audience-mapper`, only when
/// there is **exactly one**.
///
/// Zero mappers and several mappers both read as `None`, on purpose. This is
/// not "first wins": Keycloak returns a client's mappers in its own order,
/// unrelated to write order, so treating the first hit as *the* mapper would
/// mean a second one added out of band — by hand, or by anything else that
/// can reach the admin API — is invisible to every sweep. `matches` would
/// report converged while a mapper this adapter never wrote sat right next to
/// the one it did.
///
/// Collapsing "none" and "more than one" into the same `None` is safe because
/// the correction is identical either way: `declaration()` always writes the
/// full mapper set, and Keycloak's `PUT` **replaces** it rather than merging
/// (verified against a real Keycloak 26.0.8; see `docs/verification.md`), so
/// "not exactly one" is drift with the same fix as "absent" — a full rewrite
/// down to one mapper, this adapter's. There is no other drift for the
/// reconciler to notice some other way: observation stays a flat fact about
/// what is currently true, not a count for a caller to interpret.
fn audience_mapper(mappers: &[ProtocolMapperRepresentation]) -> Option<String> {
    let mut matching = mappers
        .iter()
        .filter(|mapper| mapper.protocol_mapper == AUDIENCE_MAPPER_TYPE);

    let only = matching.next()?;

    if matching.next().is_some() {
        return None;
    }

    only.config.get(AUDIENCE_MAPPER_CONFIG_KEY).cloned()
}
