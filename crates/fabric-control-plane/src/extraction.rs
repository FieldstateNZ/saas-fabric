//! Extractors that keep every refusal in this crate's error shape.
//!
//! # Why these exist rather than axum's own
//!
//! Axum's `Path` and `Json` reject in *axum's* shape — a plain-text body, or a
//! JSON body of a different form. That would mean the API documents one error
//! envelope, `{"error": {"code", "message"}}`, and answers with a different one
//! whenever a path parameter or a request body is malformed.
//!
//! A client that branches on `error.code` — which is exactly what the operator
//! console does — cannot handle a response that has no `error.code`. So both
//! rejections are translated here, in one place, rather than left to whichever
//! handler happened to be reached.

mod logical_data_source_path;

use axum::extract::{FromRequest, FromRequestParts, Request};
use fabric_client_model::{ClientId, DesiredStateError};
use fabric_core::DataSourceId;
use http::request::Parts;
use serde::de::DeserializeOwned;

pub(crate) use logical_data_source_path::LogicalDataSourcePath;

use crate::ControlPlaneError;

/// The largest request body this API will read.
///
/// A client's identity configuration is a realm name, a handful of role names,
/// and a few application clients — kilobytes at the very most. A fixed constant
/// rather than a setting, because there is no deployment for which a different
/// value would be right, and an unbounded read is how a single request becomes
/// an allocation the process cannot survive.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// The client id from the request path, already validated.
pub(crate) struct ClientPath(pub(crate) ClientId);

impl<S: Send + Sync> FromRequestParts<S> for ClientPath {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // By name out of a map, not positionally. A route with a second
        // parameter — a secret's path, say — makes a single-value extractor
        // fail with "names no client", which is a confusing way to be told
        // that a route grew.
        let axum::extract::Path(params) =
            axum::extract::Path::<std::collections::HashMap<String, String>>::from_request_parts(
                parts, state,
            )
            .await
            .map_err(|_| invalid("clientId", "the request path names no client".to_owned()))?;

        let client = params
            .get("client_id")
            .ok_or_else(|| invalid("clientId", "the request path names no client".to_owned()))?;

        // The message names the rule, not the value: a client id reaches here
        // from a URL, so echoing it back would reflect caller-controlled text
        // into a response body.
        ClientId::try_new(client)
            .map(Self)
            .map_err(|error| invalid("clientId", error.to_string()))
    }
}

/// The data source id from the request path, already validated.
///
/// # Why this is a lookup key and never a path
///
/// ADR 0023 keeps `DataSourceId` a key an adapter looks up in a document it
/// already read, never a segment it builds a repository path from. This
/// extractor is what makes that true at the boundary: a value that is not a
/// [`DataSourceId`] is refused here, before a handler ever sees it, so there
/// is no later point where an unchecked string could be handed to the
/// repository.
pub(crate) struct DataSourceIdPath(pub(crate) DataSourceId);

impl<S: Send + Sync> FromRequestParts<S> for DataSourceIdPath {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(params) =
            axum::extract::Path::<std::collections::HashMap<String, String>>::from_request_parts(
                parts, state,
            )
            .await
            .map_err(|_| invalid("dataSourceId", "the request path names no data source".to_owned()))?;

        let value = params
            .get("data_source_id")
            .ok_or_else(|| invalid("dataSourceId", "the request path names no data source".to_owned()))?;

        DataSourceId::try_new(value)
            .map(Self)
            .map_err(|error| invalid("dataSourceId", error.to_string()))
    }
}

/// A JSON request body, bounded and rejected in this crate's error shape.
pub(crate) struct BoundedJson<T>(pub(crate) T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for BoundedJson<T> {
    type Rejection = ControlPlaneError;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES)
            .await
            .map_err(|_| {
                malformed(format!(
                    "the request body could not be read; it must not exceed {MAX_BODY_BYTES} bytes"
                ))
            })?;

        serde_json::from_slice(&bytes)
            .map(Self)
            .map_err(|error| malformed(error.to_string()))
    }
}

/// A field the caller got wrong.
///
/// `pub(super)` rather than private: `logical_data_source_path` below is
/// its own file for the same reason `ClientPath` and `DataSourceIdPath`
/// are not (`docs/architecture/file-size-policy.md`'s 150-line limit),
/// and needs this to report the same way the other two path extractors
/// do.
pub(super) fn invalid(field: &'static str, detail: String) -> ControlPlaneError {
    ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField { field, detail })
}

/// A body that could not be read as what it claimed to be.
///
/// `serde_json`'s message names the offending field and position, which is the
/// most useful thing an operator can be told and names nothing internal — the
/// body is the operator's own.
///
/// `pub(crate)` rather than private: a handler that accepts part of its body
/// as a closed set of words rather than through `Deserialize` alone — the
/// data-source `placement` field, say — reports the same way a body that
/// failed to parse at all does, rather than inventing a second shape for
/// "the JSON was fine but the value was not".
pub(crate) fn malformed(detail: String) -> ControlPlaneError {
    ControlPlaneError::InvalidRequest(DesiredStateError::Malformed { detail })
}
