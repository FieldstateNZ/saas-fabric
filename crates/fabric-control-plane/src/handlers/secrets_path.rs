//! The path of a secret, taken from a request and validated before use.

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;

use fabric_client_model::DesiredStateError;

use crate::{ControlPlaneError, SecretPath};

/// A secret's path, parsed from the tail of the URL.
///
/// A wildcard segment, so `database/primary` arrives whole rather than as a
/// single segment that cannot contain a separator. That makes validation this
/// extractor's job and not the router's: it is parsed through
/// [`SecretPath`], which refuses traversal, absolute paths and encoded
/// separators before anything downstream sees it.
pub(crate) struct SecretPathTail(pub(crate) SecretPath);

impl<S: Send + Sync> FromRequestParts<S> for SecretPathTail {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(params) =
            Path::<std::collections::HashMap<String, String>>::from_request_parts(parts, state)
                .await
                .map_err(|_| missing())?;

        let raw = params.get("secret_path").ok_or_else(missing)?;

        SecretPath::parse(raw).map(Self).map_err(|detail| {
            // The caller's mistake, so a 400 — and the message repeats the
            // rule rather than the value, so a rejected path is never echoed
            // back into a page or a log.
            ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField {
                field: "secret path",
                detail,
            })
        })
    }
}

/// The rejection for a request with no path at all.
fn missing() -> ControlPlaneError {
    ControlPlaneError::InvalidRequest(DesiredStateError::MissingField { field: "secret path" })
}
