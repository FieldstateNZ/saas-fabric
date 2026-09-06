//! Building the representation SaaS Fabric writes for a declared client.

use std::collections::BTreeMap;

use fabric_client_model::{OidcClient, RedirectStrategyKind};
use fabric_reconciliation::ProviderError;

use crate::wire::{
    AudienceMapper, NewClientRepresentation, AUDIENCE_MAPPER_CONFIG_KEY, AUDIENCE_MAPPER_TYPE,
    PKCE_CHALLENGE_METHOD_ATTRIBUTE, POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE,
};

/// Builds the representation SaaS Fabric writes for a declared client.
///
/// The same body for create and update, which is what makes an update
/// idempotent: writing the declaration twice produces the same object. That
/// idempotence now covers `protocolMappers` too — Keycloak replaces a
/// client's mapper set by name on every `PUT` rather than merging it (verified
/// against a real Keycloak 26.0.8; see `docs/verification.md`), so sending the
/// same full set here is enough and no `/protocol-mappers/models` call is
/// needed.
///
/// `audience` is the string every client's mapper asserts — see
/// [`crate::KeycloakConfig::audience`] for why it lives in this adapter's own
/// configuration rather than in the document.
///
/// # Errors
///
/// Returns [`ProviderError::Rejected`] if the client's redirect strategy is
/// `customScheme`. Model validation refuses that strategy before a document is
/// ever written (ADR 0019 §3), so reaching this arm means validation was
/// bypassed — a regression that must surface as a refusal, not as a client
/// written with no callbacks.
pub(super) fn declaration<'a>(
    client: &'a OidcClient,
    audience: &'a str,
) -> Result<NewClientRepresentation<'a>, ProviderError> {
    match client.redirect.kind() {
        RedirectStrategyKind::ClaimedHttps
        | RedirectStrategyKind::PrivateNetwork
        | RedirectStrategyKind::Development => {}
        RedirectStrategyKind::CustomScheme(_) => {
            return Err(ProviderError::Rejected {
                detail: "a customScheme client reached the adapter, which cannot write one \
                         (Lane E phase 2); this is a validation regression, not a Keycloak refusal"
                    .to_owned(),
            });
        }
    }

    let mut attributes = BTreeMap::new();
    attributes.insert(
        PKCE_CHALLENGE_METHOD_ATTRIBUTE,
        client.pkce.as_wire_value().to_owned(),
    );
    attributes.insert(POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE, "+".to_owned());

    let mut mapper_config = BTreeMap::new();
    mapper_config.insert(AUDIENCE_MAPPER_CONFIG_KEY, audience);
    mapper_config.insert("access.token.claim", "true");
    mapper_config.insert("id.token.claim", "false");

    Ok(NewClientRepresentation {
        client_id: client.id.as_str(),
        enabled: true,
        protocol: "openid-connect",
        public_client: true,
        standard_flow_enabled: true,
        redirect_uris: client
            .redirect
            .uris()
            .iter()
            .map(|uri| uri.as_str().to_owned())
            .collect(),
        attributes,
        protocol_mappers: vec![AudienceMapper {
            name: "fabric-audience",
            protocol: "openid-connect",
            protocol_mapper: AUDIENCE_MAPPER_TYPE,
            config: mapper_config,
        }],
    })
}
