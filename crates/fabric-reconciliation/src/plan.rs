//! What has to change for a provider to match a client's desired state.

#[cfg(test)]
mod baseline_tests;
mod classify;
mod diff;
#[cfg(test)]
mod diff_tests;

use fabric_client_model::{OidcClient, RealmName, RoleName};

use crate::provider::{HandEdit, HeldEdits};

pub use diff::plan;

/// One change reconciliation would make.
///
/// Naming each change rather than performing it as it is discovered buys two
/// things: the plan can be *empty*, which is how "already converged" is said
/// out loud, and the plan can be logged before it is applied, so an operator
/// reading a reconciliation event sees what the platform decided rather than
/// only what it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAction {
    /// The realm does not exist and must be created.
    CreateRealm {
        /// The display name it should carry.
        display_name: String,
    },

    /// The realm exists but is displayed under a different name.
    SetRealmDisplayName {
        /// The display name it should carry.
        display_name: String,
    },

    /// A declared realm role is missing.
    CreateRealmRole(RoleName),

    /// A declared application client is missing.
    CreateOidcClient(OidcClient),

    /// A declared application client exists but a product-owned field no
    /// longer matches its declaration.
    UpdateOidcClient {
        /// The declaration to write.
        client: OidcClient,

        /// The unsupported hand edits the write must leave in place. Empty
        /// when the plan re-asserts the projection.
        held: HeldEdits,
    },
}

/// What a plan does with an unsupported hand edit (D01-19b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandEditPolicy {
    /// Report it and leave it in place. Every ordinary pass.
    Hold,

    /// Overwrite it with the declaration. Only an operator's explicit,
    /// audited re-assertion asks for this.
    Overwrite,
}

/// Everything that has to change in one realm.
///
/// Ordered, and the order matters: the realm is created before anything is put
/// in it. Roles come before application clients for no functional reason
/// today, and consistently, so a log of two reconciliation passes over the
/// same drift can be compared line for line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPlan {
    /// The realm every action applies to.
    realm: RealmName,

    /// The changes, in the order they must be applied.
    actions: Vec<IdentityAction>,

    /// The unsupported hand edits found. Held by an ordinary plan, and the
    /// ones being overwritten by a re-assertion.
    hand_edits: Vec<HandEdit>,
}

impl IdentityPlan {
    /// Builds a plan.
    pub(crate) const fn new(
        realm: RealmName,
        actions: Vec<IdentityAction>,
        hand_edits: Vec<HandEdit>,
    ) -> Self {
        Self {
            realm,
            actions,
            hand_edits,
        }
    }

    /// The realm this plan is about.
    #[must_use]
    pub const fn realm(&self) -> &RealmName {
        &self.realm
    }

    /// The changes, in the order they must be applied.
    #[must_use]
    pub fn actions(&self) -> &[IdentityAction] {
        &self.actions
    }

    /// The unsupported hand edits the provider holds on declared clients.
    ///
    /// A plan can be converged and still carry these: nothing product-owned
    /// needs writing, and the hand edits are deliberately not written over.
    #[must_use]
    pub fn hand_edits(&self) -> &[HandEdit] {
        &self.hand_edits
    }

    /// Whether the provider already matches the desired state.
    ///
    /// The single most important question this crate answers: an empty plan is
    /// what makes a second reconciliation pass a no-op, and what makes
    /// "idempotent" a checkable claim rather than a hope.
    #[must_use]
    pub fn is_converged(&self) -> bool {
        self.actions.is_empty()
    }
}
