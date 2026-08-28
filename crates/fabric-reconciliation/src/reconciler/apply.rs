//! Turning a plan into provider calls.

use fabric_client_model::RealmName;

use crate::plan::{IdentityAction, IdentityPlan};
use crate::provider::{IdentityProvider, ProviderError};

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
pub(super) async fn apply(provider: &dyn IdentityProvider, plan: &IdentityPlan) -> Result<(), ProviderError> {
    let realm = plan.realm();

    for action in plan.actions() {
        apply_one(provider, realm, action).await?;
    }

    Ok(())
}

/// Performs one action.
async fn apply_one(
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
        IdentityAction::UpdateOidcClient(client) => provider.update_oidc_client(realm, client).await,
    }
}
