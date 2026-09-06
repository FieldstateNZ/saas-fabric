//! The issuers a defence-in-depth deployment has to name twice.

use std::collections::BTreeSet;

use fabric_identity::{IdentityConfig, TrustedIssuer};

use crate::config::TokenConfig;

/// Requires `[token].issuers` and `[identity].trusted_issuers` to name the same
/// set.
///
/// A deployment running [`TokenConfig::Validating`] states its issuers in two
/// places, for two different jobs: `[token].issuers` is the
/// signature-verification allowlist, and `[identity].trusted_issuers` is the
/// tenant binding (ADR 0019 §2). Neither crate can see the other's setting —
/// `fabric-identity` knows nothing about `TokenConfig` — so the relationship
/// belongs here, where cross-crate relationships live.
///
/// Left unchecked, an issuer in one list and not the other is a token that
/// verifies and cannot be placed, or a tenant binding for an issuer whose
/// signature nobody will accept. Both are configuration errors that should not
/// survive until a request.
///
/// # Two cases this deliberately does not refuse
///
/// **The canonical posture.** [`TokenConfig::TrustedIngress`] has no issuer
/// allowlist at all, because the edge holds it (ADR 0019 §1). There is no
/// second list, so there is nothing to diverge from.
///
/// **`[token].issuers` omitted under `validating`.** That means "do not examine
/// `iss` when verifying the signature", which is a state the configuration
/// supports and which still fails closed: the tenant binding refuses an
/// unregistered issuer regardless. Requiring the allowlist here would be this
/// check inventing a rule ADR 0019 does not state.
///
/// # The audience equality is not checked here, and cannot be
///
/// ADR 0019 §1 also requires the Data API's audience string and every
/// `IssuerRegistration.audience` in the same deployment to be one string,
/// because a client carries exactly one audience mapper. That check is **not**
/// possible in this process. The Data API's edge audience is the gateway's
/// configuration and appears nowhere in `AppConfig`; `IssuerRegistration` is
/// `fabric-fga-auth`'s, loaded by a different host (`fabric-fga-auth-api`) from
/// its own file, and `fabric-api` has no edge to that crate — deliberately.
/// So the equality is a platform obligation (ADR 0019 §G5), and inventing a
/// coupling to hold it here would mean adding a dependency for the sake of a
/// comparison one of whose operands this process still would not have.
pub(super) fn validate(token: &TokenConfig, identity: &IdentityConfig) -> Result<(), String> {
    let TokenConfig::Validating {
        issuers: Some(verified),
        ..
    } = token
    else {
        return Ok(());
    };

    let verified: BTreeSet<&str> = verified.as_slice().iter().map(String::as_str).collect();
    let bound: BTreeSet<&str> = identity
        .trusted_issuers
        .iter()
        .map(TrustedIssuer::issuer)
        .collect();

    if verified == bound {
        return Ok(());
    }

    Err(format!(
        "[token].issuers and [identity].trusted_issuers must name the same issuers: \
         {} appears only in [token].issuers, and {} only in [identity].trusted_issuers",
        describe(verified.difference(&bound).copied()),
        describe(bound.difference(&verified).copied()),
    ))
}

/// Renders one side of the difference, or says there is none.
///
/// Both sides are printed even when one is empty, because "which list is
/// missing it" is the whole diagnosis and a message that named only the
/// non-empty side would leave the reader to infer the direction.
fn describe<'a>(issuers: impl Iterator<Item = &'a str>) -> String {
    let named: Vec<&str> = issuers.collect();

    if named.is_empty() {
        "nothing".to_owned()
    } else {
        named.join(", ")
    }
}
