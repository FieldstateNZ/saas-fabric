//! Router state for the control-plane API.

use std::sync::Arc;

use axum::extract::FromRef;

use crate::operator::OperatorAuthenticator;
use crate::service::ClientService;

/// What control-plane handlers can reach.
///
/// Two things, and the absence of a third is the design: there is no identity
/// provider client here, and no repository client either. A handler can reach
/// the domain service and the operator authenticator, and that is all — so
/// there is no handler that could call Keycloak, and none that could bypass
/// the service's rules to write a document directly.
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
}

impl FromRef<ControlPlaneState> for Arc<dyn OperatorAuthenticator> {
    fn from_ref(state: &ControlPlaneState) -> Self {
        Arc::clone(&state.operators)
    }
}
