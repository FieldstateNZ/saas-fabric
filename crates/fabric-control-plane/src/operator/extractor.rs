//! The axum extractor that makes the operator a handler parameter.

use std::future::{self, Future};
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

    /// Not an `async fn`, for the same reason the runtime plane's tenant
    /// extractor is not: authenticating an operator is a header read and a
    /// lookup in an allowlist already in memory, so the future it returns is
    /// already complete and there is no suspension point in it.
    ///
    /// That matters more here than it looks. When operator authentication
    /// grows a second implementation — one that verifies a credential the
    /// platform issued — the temptation will be to call something over the
    /// network *per request*. Changing this return type is where that decision
    /// has to be made explicitly rather than arrived at.
    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        let authenticator = Arc::<dyn OperatorAuthenticator>::from_ref(state);

        future::ready(
            authenticator
                .authenticate(&parts.headers)
                .map_err(ControlPlaneError::Unauthenticated),
        )
    }
}
