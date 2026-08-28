//! Structured log events for reconciliation.
//!
//! Two rules, both from the boundary this crate holds:
//!
//! - **No provider protocol vocabulary.** These lines say realm, role, and
//!   application client. They never say `RealmRepresentation`, and they never
//!   carry an upstream response body — the adapter has already reduced that to
//!   a sanitised sentence.
//! - **No credentials, ever.** Not a token, not an administrative password,
//!   not an `Authorization` header. The provider description logged here is
//!   whatever the adapter's `describe` returns, which its own documentation
//!   requires to be credential-free.

use fabric_client_model::Client;
use fabric_core::{event_id, EventType};

use crate::plan::IdentityPlan;
use crate::provider::ProviderError;
use crate::DOMAIN_ID;

/// The provider already matched the desired state.
///
/// Debug, not info: on a healthy platform this is every client on every pass,
/// and at info it would be the only thing anybody ever saw in these logs.
pub(crate) fn already_converged(client: &Client) {
    tracing::debug!(
        event = "reconciliation.converged",
        event_id = event_id(DOMAIN_ID, EventType::Debug, 1),
        client_id = %client.id,
        realm = %client.identity.realm,
        "identity already matches desired state"
    );
}

/// A plan is about to be applied.
///
/// Info, and logged *before* the calls rather than after: if the process dies
/// mid-apply, this line is the only record of what it had decided to do.
pub(crate) fn applying(client: &Client, plan: &IdentityPlan) {
    tracing::info!(
        event = "reconciliation.applying",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        client_id = %client.id,
        realm = %plan.realm(),
        actions = plan.actions().len(),
        "converging client identity"
    );
}

/// The provider's current state could not be read.
pub(crate) fn observation_failed(client: &Client, error: &ProviderError) {
    tracing::error!(
        event = "reconciliation.observation_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 1),
        client_id = %client.id,
        realm = %client.identity.realm,
        transient = error.is_transient(),
        detail = %error,
        "could not read the identity provider's current state"
    );
}

/// A plan could not be applied.
///
/// The desired state is untouched by this: nothing in reconciliation writes to
/// the desired-state repository, so a failure here leaves Git exactly as it
/// was and the next pass re-plans from the same document.
pub(crate) fn apply_failed(client: &Client, error: &ProviderError) {
    tracing::error!(
        event = "reconciliation.apply_failed",
        event_id = event_id(DOMAIN_ID, EventType::Error, 2),
        client_id = %client.id,
        realm = %client.identity.realm,
        transient = error.is_transient(),
        detail = %error,
        "could not converge client identity"
    );
}
