//! The one identity event that is not a refusal: what this deployment
//! resolves identity with.
//!
//! Its own file because everything else in [`super`] is a rejection — a token
//! that named no tenant, an issuer no registration knows — emitted at warning
//! while a request is being turned away. This is emitted once, at startup, at
//! info, and nothing about it is attacker-influenced, so none of the rules
//! that govern the refusals apply to it. Kept beside them and it reads as a
//! sanitising rule with no value to sanitise.

use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// Records the token-reading posture at startup.
///
/// Emitted at info deliberately: whether signatures are verified is the single
/// most consequential identity setting, and it should be visible in the first
/// few lines of every deployment's logs rather than inferred from config.
pub(crate) fn reader_configured(description: &str, tenant_claim: &str) {
    tracing::info!(
        event = "identity.reader_configured",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        reader = description,
        tenant_claim,
        "identity resolution configured"
    );
}
