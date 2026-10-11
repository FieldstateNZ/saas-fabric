//! Whether a retired client's tenant is still published, from publication
//! evidence only (ADR 0021's 2026-10-11 amendment, "Withdrawal is deferred").
//!
//! Recording a tombstone is intent. Nothing withdraws a retired tenant from
//! the runtime documents yet, so this never reports a withdrawal as done:
//! the most it can say is that the tenants document the publication target
//! last accepted does not carry the tenant, and even then no reader is
//! observed applying it.

use fabric_core::TenantId;
use fabric_platform_management::PassOutcome;

use super::facts::Publication;

/// Where a retired client's tenant stands in publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WithdrawalState {
    /// The accepted tenants document still carries the tenant.
    Pending,
    /// The last publication pass was refused or failed, so the runtime
    /// keeps the documents it last accepted.
    Refused,
    /// Nothing in this process shows what is published.
    Unobserved,
    /// The accepted tenants document does not carry the tenant.
    NotPublished,
}

/// The state, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Withdrawal {
    /// Which state.
    pub(crate) state: WithdrawalState,
    /// The evidence, in an operator's words.
    pub(crate) detail: String,
}

/// What the evidence says about `tenant`.
pub(crate) fn withdrawal(tenant: Option<&TenantId>, publication: &Publication) -> Withdrawal {
    let (last_pass, published) = match publication {
        Publication::NotConfigured => return of(
            WithdrawalState::Unobserved,
            "this deployment publishes no runtime documents, so nothing shows whether the tenant is served",
        ),
        Publication::Observed { last_pass, published } => (
            last_pass.as_ref(),
            published.as_ref().map(|published| &published.tenants),
        ),
        Publication::Offered(tenants) => (None, Some(tenants)),
    };
    let last_refusal = last_pass.and_then(|last| match &last.outcome {
        PassOutcome::Refused { reason } => Some(("refused", reason)),
        PassOutcome::Failed { detail } => Some(("failed", detail)),
        _ => None,
    });
    if let Some((how, why)) = last_refusal {
        return of(
            WithdrawalState::Refused,
            &format!(
                "the last publication pass {how} ({why}), so the runtime keeps the documents it last accepted"
            ),
        );
    }
    let Some(published) = published else {
        return of(
            WithdrawalState::Unobserved,
            "no publication pass has published since the control plane started",
        );
    };

    match tenant.and_then(|tenant| published.get(tenant)) {
        Some(revision) => of(
            WithdrawalState::Pending,
            &format!(
                "the tenants document the publication target last accepted still carries this tenant, at \
                 revision {}; withdrawing a retired tenant is not built yet",
                revision.get()
            ),
        ),
        None => of(
            WithdrawalState::NotPublished,
            "the tenants document the publication target last accepted does not carry this tenant; no \
             reader is observed applying it",
        ),
    }
}

fn of(state: WithdrawalState, detail: &str) -> Withdrawal {
    Withdrawal {
        state,
        detail: detail.to_owned(),
    }
}
