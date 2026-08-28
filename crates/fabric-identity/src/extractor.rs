//! The axum extractor that makes tenant identity a handler parameter.

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

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let resolver = Arc::<IdentityResolver>::from_ref(state);
        resolver.resolve(&parts.headers)
    }
}
