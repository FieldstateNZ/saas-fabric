//! Structured log events for identity resolution.
//!
//! Wrapping emission in typed helpers keeps field names identical across call
//! sites, so a dashboard filtering on `tenant_claim` does not miss half the
//! events because one call site spelled it `claim`.
//!
//! **A value that originates inside a bearer token is logged only through
//! [`sanitise`], and never in a response body.** A token is attacker-
//! controlled input, and [`sanitise`] keeps a log line from becoming a second
//! injection surface once a value has already been refused.
//!
//! **This module is that rule's enforcement point for the whole platform.**
//! [`sanitise`] is exported rather than private because two other places log
//! a token-derived value — `readers`, and `fabric-data-api`'s refused subject
//! — and a second copy of this rule is a second answer waiting to disagree.
//!
//! Over the 120-line advisory threshold. The reason is that this is one set
//! of typed emitters for one domain's refusals, each a few lines of `tracing`
//! call behind a name and a doc comment explaining what it is safe to log and
//! why; splitting them across files would separate each event from the
//! sibling events a reader needs beside it to see the whole refusal path.

mod sanitize;

use fabric_core::{event_id, EventType, IdentifierError};

use crate::DOMAIN_ID;

pub use sanitize::{sanitise, Sanitised};

/// A token arrived with no tenant claim.
pub(crate) fn tenant_claim_missing(claim: &str) {
    tracing::warn!(
        event = "identity.tenant_claim_missing",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        tenant_claim = claim,
        "bearer token carried no tenant claim; rejecting"
    );
}

/// A token's tenant claim was present but unusable.
///
/// The rejected value itself is not logged — it is attacker-controlled, and
/// writing it into the log stream would pollute tenant-filtered queries with
/// values that are not tenants. `error`'s rendering is logged, through
/// [`sanitise`]: [`IdentifierError::DisallowedCharacter`] embeds the one
/// offending character verbatim, which is the value this function exists not
/// to log unsanitised.
pub(crate) fn tenant_claim_invalid(claim: &str, error: &IdentifierError) {
    let reason = sanitise(&error.to_string());

    tracing::warn!(
        event = "identity.tenant_claim_invalid",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        tenant_claim = claim,
        reason = %reason,
        reason_truncated = reason.truncated,
        "tenant claim is not a valid tenant identifier; rejecting"
    );
}

/// A token arrived with no issuer claim, so it names no tenant.
///
/// Refused rather than treated as unregistered: an allowlist a token can skip
/// by omitting the claim it is written against is a control that does nothing.
pub(crate) fn issuer_claim_missing(claim: &str) {
    tracing::warn!(
        event = "identity.issuer_claim_missing",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 5),
        issuer_claim = claim,
        "bearer token carried no issuer claim; rejecting"
    );
}

/// A token's issuer is in no registration.
///
/// The issuer is logged, through [`sanitise`], rather than withheld: a burst
/// of one `issuer_offered` value is the operator's only signal that the
/// gateway is admitting an issuer `identity.trusted_issuers` does not know —
/// registry drift, and the registry is the authority. `registered_issuer_count`
/// rides alongside it so a dashboard can tell "the registry is empty" apart
/// from "this one issuer is missing". Sanitised and bounded is not the same as
/// safe to echo back: this value never appears in a response.
///
/// `issuer_truncated` says whether the bound fired: without it, a crafted
/// issuer sharing its first 128 bytes with a real one is the same line.
pub(crate) fn issuer_unregistered(issuer: &str, registered_issuer_count: usize) {
    let issuer = sanitise(issuer);

    tracing::warn!(
        event = "identity.issuer_unregistered",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 6),
        issuer_offered = %issuer,
        issuer_truncated = issuer.truncated,
        registered_issuer_count,
        "bearer token's issuer is not registered; rejecting"
    );
}

/// A token's tenant claim named a tenant its issuer does not own.
///
/// Worth noticing above every other refusal here: it is either a
/// cross-tenant attempt, or the edge and this registry disagreeing about which
/// tenant an issuer serves. Neither value is logged; both are in the token.
pub(crate) fn tenant_claim_disagrees_with_issuer(claim: &str) {
    tracing::warn!(
        event = "identity.tenant_claim_disagrees_with_issuer",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 7),
        tenant_claim = claim,
        "tenant claim does not name the tenant its issuer is registered to; rejecting"
    );
}

/// A caller attempted tenant selection through the banned header.
///
/// Logged at warning because it is worth noticing: it is either a client built
/// against the wrong contract, or someone probing for cross-tenant access.
pub(crate) fn tenant_header_rejected(header: &str) {
    tracing::warn!(
        event = "identity.tenant_header_rejected",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        header,
        "request attempted tenant selection through a header; rejecting"
    );
}

/// The banned header was present while rejection is switched off.
pub(crate) fn tenant_header_ignored(header: &str) {
    tracing::warn!(
        event = "identity.tenant_header_ignored",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 4),
        header,
        "request carried a tenant header; ignoring it — the tenant comes from the bearer token"
    );
}

/// Records the token-reading posture at startup.
///
/// Emitted at info deliberately: whether signatures are verified is the single
/// most consequential identity setting, and it should be visible in the first
/// few lines of every deployment's logs rather than inferred from config.
pub fn reader_configured(description: &str, tenant_claim: &str) {
    tracing::info!(
        event = "identity.reader_configured",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        reader = description,
        tenant_claim,
        "identity resolution configured"
    );
}
