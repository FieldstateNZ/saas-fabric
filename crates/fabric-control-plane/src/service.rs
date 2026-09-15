//! The control plane's domain operations.
//!
//! In the 121–150 line band. The reason is that this is one struct,
//! [`ClientService`], its four fields' own rustdoc — which is where the
//! reasoning for two of them, `reserved_realms` and `reserved_client_ids`,
//! actually lives — together with its constructor and its two plain reads,
//! `list` and `get`. Every rule that *does* something to a client is
//! already one file per rule (`create_client.rs`, `set_product.rs`, and the
//! rest); what stays here is the struct those files all share and the
//! handful of methods too small to be their own rule.

mod catalogue;
mod create_client;
mod realm_available;
mod reconciliation_view;
#[cfg(test)]
mod reconciliation_view_tests;
mod reserved;
#[cfg(test)]
mod service_tests;
mod set_identity;
mod set_product;
mod state_access;

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::ClientId;
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
    ///
    /// Plain, case-folded strings, not [`RealmName`](fabric_client_model::RealmName):
    /// parsing a Keycloak realm name through `RealmName`'s strict DNS-label
    /// rule could fail a deployment's startup over a realm name Keycloak
    /// itself had always accepted. See
    /// `fabric_control_plane_api::startup::reserved_names::realms` for the
    /// full argument, and where these are actually computed.
    reserved_realms: Arc<BTreeSet<String>>,

    /// Application ids the catalogue may never accept. Plain strings, not
    /// [`ClientId`]: this deployment's own console or Keycloak adapter
    /// `client_id` is arbitrary configuration this crate does not choose,
    /// so forcing it through `ClientId::try_new` risks the same startup
    /// failure `reserved_realms` avoids above. Computed the same way, and
    /// for the same reason, as `reserved_realms`.
    reserved_client_ids: Arc<BTreeSet<String>>,
}

impl ClientService {
    /// Assembles the service.
    #[must_use]
    pub fn new(
        repository: Arc<DesiredStateBinding>,
        reconciliation: Arc<ReconciliationStatusStore>,
        clock: Arc<dyn Clock>,
        reserved_realms: Arc<BTreeSet<String>>,
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
