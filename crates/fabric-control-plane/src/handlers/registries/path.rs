//! A registry's host, and a repository under it, taken from the request path
//! and validated before any handler sees them.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use http::request::Parts;

use crate::{ControlPlaneError, RegistryFailure, RegistryHost, Repository};

/// The registry's host from the path.
///
/// # A lookup key, never a location
///
/// It selects a record this platform already holds, the way
/// `DataSourceIdPath` selects a declared data source: a value that is not a
/// [`RegistryHost`] is refused here, so no later code meets an unchecked
/// string, and none builds a URL, a store path or a secret name from it.
pub(crate) struct RegistryHostPath(pub(crate) RegistryHost);

impl<S: Send + Sync> FromRequestParts<S> for RegistryHostPath {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let params = params(parts, state).await?;
        let host = params
            .get("host")
            .ok_or_else(|| invalid("the request path names no registry"))?;
        RegistryHost::parse(host)
            .map(Self)
            .map_err(|rule| ControlPlaneError::Registry(RegistryFailure::Invalid(rule)))
    }
}

/// A repository under the path's registry: `{host}/{path}`.
///
/// A wildcard tail, so `fieldstatenz/saas-fabric` arrives whole; parsed by
/// the one rule every image reference is held to, so a repository of one
/// Docker Hub segment is refused naming `docker.io/library/<name>`.
pub(crate) struct RepositoryTail(pub(crate) Repository);

impl<S: Send + Sync> FromRequestParts<S> for RepositoryTail {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let params = params(parts, state).await?;
        let (Some(host), Some(path)) = (params.get("host"), params.get("path")) else {
            return Err(invalid("the request path names no repository"));
        };
        Repository::try_new(format!("{host}/{path}"))
            .map(Self)
            .map_err(|error| ControlPlaneError::Registry(RegistryFailure::Invalid(error.to_string())))
    }
}

/// The path's parameters, by name.
async fn params<S: Send + Sync>(
    parts: &mut Parts,
    state: &S,
) -> Result<HashMap<String, String>, ControlPlaneError> {
    Path::<HashMap<String, String>>::from_request_parts(parts, state)
        .await
        .map(|Path(params)| params)
        .map_err(|_| invalid("the request path could not be read"))
}

/// A path that names nothing usable.
fn invalid(message: &str) -> ControlPlaneError {
    ControlPlaneError::Registry(RegistryFailure::Invalid(message.to_owned()))
}
