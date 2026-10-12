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

mod withheld;

use fabric_client_model::Client;
use fabric_core::{event_id, EventType};

use crate::plan::IdentityPlan;
use crate::provider::{HandEdit, ProviderError};
use crate::DOMAIN_ID;

pub(crate) use withheld::clients_withheld;

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
/// `overwriting` is non-empty only on an operator's re-assertion, and names
/// the hand edits it is about to write over.
pub(crate) fn applying(client: &Client, plan: &IdentityPlan, overwriting: &[HandEdit]) {
    tracing::info!(
        event = "reconciliation.applying",
        event_id = event_id(DOMAIN_ID, EventType::Success, 1),
        client_id = %client.id,
        realm = %plan.realm(),
        actions = plan.actions().len(),
        overwriting = %joined(overwriting),
        "converging client identity"
    );
}

/// The provider holds hand edits the product does not support, and this pass
/// left them in place (D01-19b).
///
/// Warn: nothing failed, but the provider does not hold what was declared
/// and nothing will change that until somebody acts.
pub(crate) fn hand_edits_held(client: &Client, held: &[HandEdit]) {
    tracing::warn!(
        event = "reconciliation.hand_edits_held",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 1),
        client_id = %client.id,
        realm = %client.identity.realm,
        hand_edits = %joined(held),
        "unsupported hand edits left in place; remove them or re-assert the projection"
    );
}

/// `web:clientDisabled,web:standardFlowDisabled`, for one log field.
fn joined(hand_edits: &[HandEdit]) -> String {
    hand_edits
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
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

/// The provider was read successfully, but names no audience — a
/// configuration fault, not a failure to observe, so it gets its own event
/// rather than sharing [`observation_failed`]'s: the two are refused for
/// different reasons and an operator reading the log should not have to
/// guess which.
pub(crate) fn planning_refused(client: &Client, error: &ProviderError) {
    tracing::error!(
        event = "reconciliation.planning_refused",
        event_id = event_id(DOMAIN_ID, EventType::Error, 3),
        client_id = %client.id,
        realm = %client.identity.realm,
        transient = error.is_transient(),
        detail = %error,
        "the provider names no audience; nothing was planned"
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
