//! The control plane's domain operations.
//!
//! Just over the 120-line advisory threshold. The reason is that this is one
//! struct, [`ClientService`], together with its constructor and its trivial
//! accessors — the struct's own doc below says why every rule about a client
//! lives in exactly one copy here; splitting its methods across files would
//! be splitting rules that exist specifically so they cannot disagree.

mod catalogue;
mod create_client;
mod reconciliation_view;
#[cfg(test)]
mod reconciliation_view_tests;
mod reserved;
#[cfg(test)]
mod service_tests;
mod set_identity;
mod set_product;

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::{ClientId, RealmName};
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::models::ReconciliationResponse;
use crate::repository::{DesiredStateBinding, StoredClient};
use crate::ControlPlaneError;

/// Everything an operator can do to a client, expressed once.
///
/// # Handlers are thin because this is where the rules are
///
/// An HTTP handler in this crate parses a path parameter, calls one method
/// here, and renders the result. Every rule that matters — that a realm cannot
/// move, that a write states the revision it edits, that a write marks
/// reconciliation pending, that every mutation is attributed — lives in this
/// module. A rule enforced in a handler is a rule the next handler will not
/// have.
///
/// # What it cannot reach
///
/// Not Keycloak, not any platform service. The only two things it holds are
/// the desired-state repository and what is known about reconciliation. That
/// is not a coincidence of the current wiring: it is the structural form of
/// ADR 0008, and it is why no operator action can bypass Git.
pub struct ClientService {
    /// Where desired state lives — or the fact that it does not yet.
    ///
    /// A binding rather than a repository, because the platform starts without
    /// one and an operator connects it later. Every method reads the current
    /// binding, so a connection made while the process runs takes effect on
    /// the next operation rather than at the next restart.
    repository: Arc<DesiredStateBinding>,

    /// What is known about whether desired state has taken effect.
    reconciliation: Arc<ReconciliationStatusStore>,

    /// Stamps audit and reconciliation records.
    clock: Arc<dyn Clock>,

    /// Realms a new client may never declare. Computed at startup by
    /// whoever assembles this deployment — see
    /// [`create_client`](Self::create_client) for what it protects.
    reserved_realms: Arc<BTreeSet<RealmName>>,

    /// Application ids the catalogue may never accept. Plain strings, not
    /// [`ClientId`] — see `service::catalogue::change_catalogue` for why —
    /// computed the same way, and for the same reason, as `reserved_realms`.
    reserved_client_ids: Arc<BTreeSet<String>>,
}

impl ClientService {
    /// What is known about whether desired state has taken effect.
    ///
    /// Exposed for the convergence pass, which records into the same store the
    /// read paths report from — two stores would be two answers to one
    /// question.
    #[must_use]
    pub(crate) fn statuses(&self) -> &ReconciliationStatusStore {
        &self.reconciliation
    }

    /// The clock, so a pass stamps outcomes the same way a write does.
    #[must_use]
    pub(crate) fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// Assembles the service.
    #[must_use]
    pub fn new(
        repository: Arc<DesiredStateBinding>,
        reconciliation: Arc<ReconciliationStatusStore>,
        clock: Arc<dyn Clock>,
        reserved_realms: Arc<BTreeSet<RealmName>>,
        reserved_client_ids: Arc<BTreeSet<String>>,
    ) -> Self {
        Self {
            repository,
            reconciliation,
            clock,
            reserved_realms,
            reserved_client_ids,
        }
    }

    /// Every client the platform manages.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the repository could not be read. A
    /// repository holding no clients is an empty list, not an error.
    pub async fn list(&self) -> Result<Vec<StoredClient>, ControlPlaneError> {
        self.repository
            .current()
            .list()
            .await
            .map_err(ControlPlaneError::from_repository)
    }

    /// One client's desired state.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError::UnknownClient`] if there is no such
    /// client, or another variant if the repository could not be read or holds
    /// a document that will not parse.
    pub async fn get(&self, client: &ClientId) -> Result<StoredClient, ControlPlaneError> {
        self.repository
            .current()
            .get(client)
            .await
            .map_err(ControlPlaneError::from_repository)
    }

    /// Where reconciliation stands for a client, as of the revision given.
    pub(crate) fn reconciliation(&self, stored: &StoredClient) -> ReconciliationResponse {
        let report = self.reconciliation.report(&stored.document.client().id);

        reconciliation_view::resolve(report.as_ref(), &stored.revision)
    }
}
