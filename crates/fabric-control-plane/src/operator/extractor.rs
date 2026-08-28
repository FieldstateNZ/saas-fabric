//! The axum extractor that makes the operator a handler parameter.

use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use http::request::Parts;

use crate::operator::{Operator, OperatorAuthenticator};
use crate::ControlPlaneError;

/// Extracts the authenticated operator from the request.
///
/// The same device the runtime plane uses for tenant identity, for the same
/// reason: a handler that takes an [`Operator`] parameter cannot run without
/// one, so there is no path into control-plane logic that skipped
/// authentication. Every handler in this crate takes one — including the read
/// handlers, because who is reading a client's configuration is worth
/// recording too.
impl<S> FromRequestParts<S> for Operator
where
    S: Send + Sync,
    Arc<dyn OperatorAuthenticator>: FromRef<S>,
{
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let authenticator = Arc::<dyn OperatorAuthenticator>::from_ref(state);

        authenticator
            .authenticate(&parts.headers)
            .map_err(ControlPlaneError::Unauthenticated)
    }
}
