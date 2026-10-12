//! Sorting a declared client's differences into product-owned drift and
//! unsupported hand edits (D01-19b).
//!
//! | Observed term | Class | Why |
//! |---|---|---|
//! | confidential instead of public | product-owned | changes what the client *is*; breaks every browser flow |
//! | a redirect URI the model cannot parse | product-owned | ADR 0019 §6: attacker-influenced, never held |
//! | a redirect URI nobody declared | product-owned | widens where codes are sent (D13b) |
//! | PKCE method other than the declared one | product-owned | a downgrade |
//! | audience mapper missing, wrong or doubled | product-owned | the edge's `aud` check (ADR 0019 §G5) |
//! | a client-level mapper nobody declared | product-owned | can inject claims |
//! | post-logout set no longer "every registered URI" | product-owned | the adapter cannot tell a narrowing from a widening, so it is never held |
//! | client disabled | **held** | narrows; may be incident containment |
//! | standard flow disabled | **held** | narrows |
//! | a declared redirect URI removed | **held** | narrows |
//! | a declared redirect URI not yet written | product-owned | a transition the operator made (D01-15) |
//! | a declared redirect URI nobody can place | **held, whole client** | see below |
//!
//! Which of the last three a missing declared callback is comes from the
//! [`AppliedBaseline`]: confirmed written before means removed by hand; never
//! recorded means not yet written; recorded as being written but never
//! confirmed, or no baseline at all, means nobody can say. That last case
//! writes nothing to the client, so its old callbacks are not removed while
//! its new ones are withheld, and an operator's re-assertion decides.
//!
//! Everything the adapter neither reads nor writes — description, web
//! origins, consent, client scopes and their mappers, token lifespans,
//! themes — is **unmanaged and not evaluated**. No term observes it and no
//! write carries it, so nothing here attests that it is safe: an edit to it,
//! widening or not, is invisible to reconciliation (ADR 0019 defers web
//! origins; a scope's mappers are a documented limit).
//!
//! The rule behind the table, for the managed fields only: an edit that
//! widens what a client can do is not left in place, because leaving it is
//! leaving a weaker posture than the one declared. An edit that only narrows
//! it is somebody's decision the product does not support, and overwriting it
//! on every pass is the "silently fighting" D01-19b refuses.

use std::collections::BTreeSet;

use fabric_client_model::{OidcClient, RedirectUri};

use crate::ledger::{CallbackDigest, WrittenCallbacks};
use crate::provider::{HeldEdits, ObservedOidcClient};

/// How an existing client differs from its declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Comparison {
    /// Whether a product-owned field differs, so the client must be written.
    pub(super) drift: bool,

    /// The unsupported hand edits it carries.
    pub(super) held: HeldEdits,

    /// Whether a declared callback is missing that nothing durable places:
    /// the client must not be written under an ordinary pass.
    pub(super) unconfirmed: bool,
}

/// Compares one existing client with its declaration, against what the
/// ledger says was written for it (`None`: nothing is known).
pub(super) fn compare(
    declared: &OidcClient,
    existing: &ObservedOidcClient,
    configured_audience: &str,
    written: Option<&WrittenCallbacks>,
) -> Comparison {
    let declared_uris: BTreeSet<RedirectUri> = declared.redirect.uris().iter().cloned().collect();

    let undeclared_uri = existing.redirect_uris.difference(&declared_uris).next().is_some();

    let mut removed_redirect_uris = BTreeSet::new();
    let mut not_yet_written = false;
    let mut unconfirmed = false;
    for missing in declared_uris.difference(&existing.redirect_uris) {
        let digest = CallbackDigest::of(missing);
        match written {
            Some(written) if written.confirmed.contains(&digest) => {
                removed_redirect_uris.insert(missing.clone());
            }
            Some(written) if !written.unconfirmed.contains(&digest) => not_yet_written = true,
            _ => unconfirmed = true,
        }
    }

    let drift = !existing.public
        || existing.unmodellable_redirect_uris != 0
        || undeclared_uri
        || not_yet_written
        || existing.challenge_method != Some(declared.pkce)
        || existing.audience_mapper.as_deref() != Some(configured_audience)
        || existing.other_protocol_mappers != 0
        || !existing.post_logout_redirect_uris_is_every_registered_uri;

    let held = HeldEdits {
        client_disabled: !existing.enabled,
        standard_flow_disabled: !existing.standard_flow_enabled,
        removed_redirect_uris,
    };

    Comparison {
        drift,
        held,
        unconfirmed,
    }
}
