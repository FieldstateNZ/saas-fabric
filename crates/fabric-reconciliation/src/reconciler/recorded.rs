//! Recording a client write in the declaration ledger on both sides of the
//! provider call (D01-15). See the crate's `ledger` module for why.

use fabric_client_model::{Client, OidcClient, RealmName};

use super::apply::apply_one;
use crate::ledger::{AppliedBaseline, DeclarationLedger, LedgerError, WriteStage};
use crate::plan::IdentityAction;
use crate::provider::{IdentityProvider, ProviderError};

/// Why an action did not complete: the provider refused it, or the ledger
/// could not record it, in which case the provider was not called for it
/// (`Writing`) or its effect stands unrecorded (`Written`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ApplyError {
    /// The provider call failed.
    #[error(transparent)]
    Provider(#[from] ProviderError),

    /// The ledger record around a client write failed.
    #[error(transparent)]
    Unrecorded(#[from] LedgerError),
}

impl ApplyError {
    /// Whether another attempt could plausibly succeed unaided.
    pub(crate) const fn is_transient(&self) -> bool {
        match self {
            Self::Provider(error) => error.is_transient(),
            Self::Unrecorded(_) => true,
        }
    }
}

/// Where client writes are recorded, and what was already recorded.
pub(super) struct Recording<'a> {
    /// The ledger, if this deployment keeps one.
    pub(super) ledger: Option<&'a dyn DeclarationLedger>,
    /// The client the plan is for.
    pub(super) client: &'a Client,
    /// What the ledger held when the plan was made.
    pub(super) baseline: &'a AppliedBaseline,
}

/// Performs one action, recording a client write in the ledger on both
/// sides of it when the baseline does not already say exactly this.
///
/// On failure, also whether the provider write had already taken effect.
pub(super) async fn recorded(
    provider: &dyn IdentityProvider,
    realm: &RealmName,
    action: &IdentityAction,
    recording: &Recording<'_>,
) -> Result<(), (ApplyError, bool)> {
    let ledger = recording
        .ledger
        .zip(written_client(action).filter(|declared| recording.baseline.must_record(declared)));

    if let Some((ledger, declared)) = ledger {
        ledger
            .record(recording.client, declared, WriteStage::Writing)
            .await
            .map_err(|error| (error.into(), false))?;
    }
    apply_one(provider, realm, action)
        .await
        .map_err(|error| (error.into(), false))?;
    if let Some((ledger, declared)) = ledger {
        ledger
            .record(recording.client, declared, WriteStage::Written)
            .await
            .map_err(|error| (error.into(), true))?;
    }

    Ok(())
}

/// The declared client an action writes, if it writes one.
const fn written_client(action: &IdentityAction) -> Option<&OidcClient> {
    match action {
        IdentityAction::CreateOidcClient(client) | IdentityAction::UpdateOidcClient { client, .. } => {
            Some(client)
        }
        _ => None,
    }
}
