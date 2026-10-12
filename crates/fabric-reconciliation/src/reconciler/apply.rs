//! Turning a plan into provider calls.

use fabric_client_model::RealmName;

use crate::plan::{IdentityAction, IdentityPlan};
use crate::provider::{HandEdit, IdentityProvider, ProviderError};

use super::recorded::recorded;
pub(crate) use super::recorded::ApplyError;
pub(super) use super::recorded::Recording;

/// A plan that stopped part-way.
///
/// Carries how many actions completed before the failing one, because
/// those took effect: a caller that reported a failed pass as "nothing
/// changed" would be wrong about every one of them.
pub(super) struct Interrupted {
    /// How many actions, from the start of the plan, the provider confirmed.
    pub(super) completed: usize,

    /// Why the next one failed. Whether it took effect is unknown, unless
    /// `recorded_late` says otherwise.
    pub(super) error: ApplyError,

    /// The failing action's provider write was confirmed and only its
    /// `Written` record failed: it is counted in `completed`, and nothing is
    /// left unconfirmed by it in this operation's overwrite report. Whether its
    /// callbacks stay unplaceable afterwards holds only while no concurrent
    /// operation completes on the same client (see the `ledger` module).
    pub(super) recorded_late: bool,
}

/// Applies every action in order, stopping at the first failure.
///
/// # Why it stops rather than continuing
///
/// The actions are ordered because they depend on each other — a role cannot
/// be created in a realm that does not exist. Continuing past a failure would
/// generate a cascade of secondary errors whose first line is not the actual
/// problem, and would report the count of "applied" changes as larger than the
/// number that took effect.
///
/// Stopping leaves the provider partly converged, which is safe here for a
/// reason worth stating: every action is additive and idempotent, so the next
/// pass re-plans against what actually exists and continues from there. There
/// is no half-applied state that a retry would make worse.
pub(super) async fn apply(
    provider: &dyn IdentityProvider,
    plan: &IdentityPlan,
    recording: &Recording<'_>,
) -> Result<(), Interrupted> {
    let realm = plan.realm();

    for (completed, action) in plan.actions().iter().enumerate() {
        recorded(provider, realm, action, recording)
            .await
            .map_err(|(error, recorded_late)| Interrupted {
                completed: completed + usize::from(recorded_late),
                error,
                recorded_late,
            })?;
    }

    Ok(())
}

/// Splits the hand edits a re-assertion set out to overwrite by what is
/// known of each after the plan stopped at `completed` actions.
///
/// Returns `(confirmed, unconfirmed)`: hand edits on a client whose update
/// the provider confirmed, and those on the client whose update failed —
/// which may or may not have taken effect, so it is claimed as neither.
/// Edits on clients the plan never reached are in neither list: they are
/// provably still in place.
pub(super) fn overwrites_after(
    plan: &IdentityPlan,
    completed: usize,
    overwriting: &[HandEdit],
) -> (Vec<HandEdit>, Vec<HandEdit>) {
    let updated = |actions: &[IdentityAction]| -> Vec<HandEdit> {
        overwriting
            .iter()
            .filter(|edit| {
                actions.iter().any(|action| {
                    matches!(action, IdentityAction::UpdateOidcClient { client, .. } if client.id == edit.client)
                })
            })
            .cloned()
            .collect()
    };

    let confirmed = updated(plan.actions().get(..completed).unwrap_or_default());
    let failing = updated(plan.actions().get(completed..=completed).unwrap_or_default());

    (confirmed, failing)
}

/// Performs one action.
pub(super) async fn apply_one(
    provider: &dyn IdentityProvider,
    realm: &RealmName,
    action: &IdentityAction,
) -> Result<(), ProviderError> {
    match action {
        IdentityAction::CreateRealm { display_name } => provider.create_realm(realm, display_name).await,
        IdentityAction::SetRealmDisplayName { display_name } => {
            provider.set_realm_display_name(realm, display_name).await
        }
        IdentityAction::CreateRealmRole(role) => provider.create_realm_role(realm, role).await,
        IdentityAction::CreateOidcClient(client) => provider.create_oidc_client(realm, client).await,
        IdentityAction::UpdateOidcClient { client, held } => {
            provider.update_oidc_client(realm, client, held).await
        }
    }
}
