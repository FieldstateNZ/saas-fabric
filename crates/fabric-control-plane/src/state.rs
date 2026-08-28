//! Router state for the control-plane API.

use std::sync::Arc;

use axum::extract::FromRef;

use crate::operator::OperatorAuthenticator;
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
}

impl FromRef<ControlPlaneState> for Arc<dyn OperatorAuthenticator> {
    fn from_ref(state: &ControlPlaneState) -> Self {
        Arc::clone(&state.operators)
    }
}
