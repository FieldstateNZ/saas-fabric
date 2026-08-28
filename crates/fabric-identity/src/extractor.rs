//! The axum extractor that makes tenant identity a handler parameter.

use std::future::{self, Future};
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use http::request::Parts;

use crate::{IdentityError, IdentityResolver, TenantIdentity};

/// Extracts the tenant identity context from the request.
///
/// Being an extractor is the point: a handler that takes a [`TenantIdentity`]
/// parameter cannot run without one, so "did we remember to check the tenant?"
/// stops being a review question and becomes a compile-time one. There is no
/// way to reach handler code with an unresolved tenant, and no ambient
/// "current tenant" for a stray code path to read.
///
/// The resolver is pulled from router state through [`FromRef`], so the
/// extractor works with any state type that can produce one.
///
/// # Examples
///
/// ```ignore
/// async fn list_customers(identity: TenantIdentity) -> impl IntoResponse {
///     // `identity.tenant()` is the only tenant this request can ever mean.
/// }
/// ```
impl<S> FromRequestParts<S> for TenantIdentity
where
    S: Send + Sync,
    Arc<IdentityResolver>: FromRef<S>,
{
    type Rejection = IdentityError;

    /// Not an `async fn`, and the difference is not cosmetic.
    ///
    /// Resolving a tenant is pure: it reads headers, decodes a token, and
    /// checks claims against keys already in memory. There is nothing to await
    /// — deliberately, because §6 keeps Git, Kubernetes and every other remote
    /// lookup off the request path, and an extractor that could await is an
    /// extractor somebody could put a network call inside.
    ///
    /// Returning an already-complete future says that at the call site: the
    /// work happens before the future is built, so there is no suspension
    /// point for anything to be added to. `async fn` here would compile a
    /// state machine that never yields, and quietly leave that door open.
    ///
    /// `impl Future` rather than the concrete [`Ready`](std::future::Ready):
    /// naming it would refine the trait's return type into this crate's public
    /// API, committing to a future shape nobody needs.
    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        let resolver = Arc::<IdentityResolver>::from_ref(state);

        future::ready(resolver.resolve(&parts.headers))
    }
}
