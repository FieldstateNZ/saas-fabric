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

    /// Builds a provider carrying an operator's authority, when this
    /// deployment converges an identity provider at all.
    ///
    /// `None` for the development provider, which converges nothing.
    pub(crate) identity_provider: Option<Arc<dyn crate::IdentityProviderFactory>>,

    /// Where this control plane is reachable from a browser.
    pub(crate) public_base_url: String,

    /// One client's secrets, when a deployment has a store for them.
    pub(crate) client_secrets: Option<Arc<crate::SecretsService>>,

    /// The flow that connects the platform repository, when this deployment
    /// manages one.
    ///
    /// A second service over the same stores, not a second use of the first.
    /// It holds its own application, its own installation and its own record,
    /// and the two share nothing an operator could confuse: connecting one
    /// does not connect the other, and forgetting one leaves the other alone.
    pub(crate) platform_integration: Option<Arc<GitIntegrationService>>,

    /// Platform Management, when this deployment has a platform repository.
    ///
    /// `None` where nothing is connected. The route is still mounted and says
    /// so, for the same reason the secrets routes are: a console can tell an
    /// operator what is missing, and cannot tell them anything about a route
    /// that does not exist.
    pub(crate) platform: Option<crate::PlatformBinding>,

    /// What the last sweep found, and whether one is running.
    ///
    /// Shared with the loop the host starts. Read here, written there.
    pub(crate) platform_sweeps: Arc<fabric_platform_management::SweepState>,
}

impl ControlPlaneState {
    /// Platform Management, or a refusal naming why there is none.
    pub(crate) fn platform(&self) -> Result<&crate::PlatformBinding, crate::ControlPlaneError> {
        self.platform
            .as_ref()
            .ok_or(crate::ControlPlaneError::PlatformNotManaged)
    }

    /// The secrets service, or a refusal naming why there is none.
    pub(crate) fn secrets(&self) -> Result<&Arc<crate::SecretsService>, crate::ControlPlaneError> {
        self.client_secrets
            .as_ref()
            .ok_or(crate::ControlPlaneError::Secrets(crate::SecretsError::NoBoundary))
    }
}

impl FromRef<ControlPlaneState> for Arc<dyn OperatorAuthenticator> {
    fn from_ref(state: &ControlPlaneState) -> Self {
        Arc::clone(&state.operators)
    }
}
