//! Router state for the control-plane API.

use std::sync::Arc;

use axum::extract::FromRef;

use crate::git_integration::GitIntegrationService;
use crate::integration::IntegrationHealth;
use crate::operator::OperatorAuthenticator;
use crate::repository::DesiredStateBinding;
use crate::service::ClientService;
use crate::sign_in::SignInSurface;

/// What control-plane handlers can reach.
///
/// What is *absent* is the design: there is no identity provider client here
/// and no repository client either, so there is no handler that could call
/// Keycloak to change a realm, and none that could bypass the service's rules
/// to write a document directly.
///
/// The sign-in surface is not an exception to that. It redeems an
/// authorization code and returns a token; it cannot read or change anything
/// about a client, and no handler that touches desired state can reach it.
#[derive(Clone)]
pub(crate) struct ControlPlaneState {
    /// The domain operations.
    pub(crate) service: Arc<ClientService>,

    /// Establishes which operator a request belongs to.
    ///
    /// Held here so that [`Operator`](crate::Operator) works as an extractor —
    /// it is pulled out through [`FromRef`]. That indirection is what lets a
    /// handler declare an `Operator` parameter and be unable to run without an
    /// authenticated one.
    pub(crate) operators: Arc<dyn OperatorAuthenticator>,

    /// How an operator obtains a token, when this posture has a sign-in at all.
    ///
    /// `None` under the trusted-header posture, where the network boundary has
    /// already authenticated the operator and there is nothing to sign in to.
    /// The session routes are then not mounted, so this is never `None` behind
    /// a handler that needs it.
    pub(crate) sign_in: Option<Arc<SignInSurface>>,

    /// Whether desired state is connected, for the integration status only.
    ///
    /// The *binding*, not a repository: this is the one caller that needs to
    /// ask whether anything is bound rather than to use what is. Client
    /// handlers go through [`ClientService`], which is what keeps the rules in
    /// one place.
    pub(crate) desired_state: Arc<DesiredStateBinding>,

    /// What the last sweep observed about reading desired state.
    pub(crate) health: Arc<IntegrationHealth>,

    /// The connection flow, when this deployment has one.
    ///
    /// `None` where desired state is stated by the deployment rather than
    /// connected by an operator: there is nothing to connect, and offering a
    /// flow that would overwrite a stated repository would be offering to
    /// undo the deployment.
    pub(crate) git_integration: Option<Arc<GitIntegrationService>>,

    /// Where this control plane is reachable from a browser.
    pub(crate) public_base_url: String,
}

impl ControlPlaneState {
    /// The connection flow, or a refusal naming why there is none.
    pub(crate) fn git_integration(&self) -> Result<&Arc<GitIntegrationService>, crate::ControlPlaneError> {
        self.git_integration
            .as_ref()
            .ok_or(crate::ControlPlaneError::IntegrationNotManaged)
    }
}

impl FromRef<ControlPlaneState> for Arc<dyn OperatorAuthenticator> {
    fn from_ref(state: &ControlPlaneState) -> Self {
        Arc::clone(&state.operators)
    }
}
