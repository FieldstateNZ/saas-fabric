//! Reading what a client's protocol-mapper list says about its audience.
//!
//! Split out of `clients.rs`, for the same reason `wire/protocol_mapper.rs`
//! split from `wire/oidc_client.rs`: a client's mapper list is one Keycloak
//! concept, but this model needs two related facts out of it, both found by
//! walking the same short list once. Splitting them into two files would
//! scatter one comparison across two, not separate two concepts.

use crate::wire::{ProtocolMapperRepresentation, AUDIENCE_MAPPER_CONFIG_KEY, AUDIENCE_MAPPER_TYPE};

/// Reads the two facts this model needs from a client's protocol-mapper
/// list: the configured audience of its one audience mapper, and how many of
/// its other mappers are not that one.
///
/// # The audience: `None` for zero mappers and for several, alike
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
/// down to one mapper, this adapter's.
///
/// # The count: any mapper this adapter did not declare is drift too
///
/// A client-level mapper that is not the audience mapper — a hardcoded-claim
/// mapper injecting a claim, say — is a mapper nobody here declared. Reported
/// as a count for the same reason an unmodellable redirect URI is (see
/// `clients::partition_uris`): correcting it never needs to know what it
/// was, only that it should not be there. Client-level only, never a client
/// scope's: observed on Keycloak 26.0.8, a freshly written client carries
/// exactly the one mapper, so scanning the client's own list is enough.
pub(super) fn read(mappers: &[ProtocolMapperRepresentation]) -> (Option<String>, usize) {
    let mut audience_mappers = mappers
        .iter()
        .filter(|mapper| mapper.protocol_mapper == AUDIENCE_MAPPER_TYPE);

    let audience = match audience_mappers.next() {
        Some(only) if audience_mappers.next().is_none() => {
            only.config.get(AUDIENCE_MAPPER_CONFIG_KEY).cloned()
        }
        _ => None,
    };

    let other = mappers
        .iter()
        .filter(|mapper| mapper.protocol_mapper != AUDIENCE_MAPPER_TYPE)
        .count();

    (audience, other)
}
