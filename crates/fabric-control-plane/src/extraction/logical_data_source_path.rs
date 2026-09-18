//! The logical data source name from the request path, already validated.

use axum::extract::FromRequestParts;
use fabric_core::LogicalDataSourceName;
use http::request::Parts;

use crate::extraction::invalid;
use crate::ControlPlaneError;

/// The logical data source name from the request path, already validated.
///
/// `PlacementRecord`'s sibling to `DataSourceIdPath`: `LogicalDataSourceName`
/// is a key the selector looks up in a client's `spec.data`, and a
/// caller-supplied value that failed to parse must never reach that lookup
/// unchecked -- see `ClientPath` and `DataSourceIdPath` for the same rule
/// applied to the other two path-borne identifiers this API reads.
pub(crate) struct LogicalDataSourcePath(pub(crate) LogicalDataSourceName);

impl<S: Send + Sync> FromRequestParts<S> for LogicalDataSourcePath {
    type Rejection = ControlPlaneError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(params) =
            axum::extract::Path::<std::collections::HashMap<String, String>>::from_request_parts(
                parts, state,
            )
            .await
            .map_err(|_| {
                invalid(
                    "logical",
                    "the request path names no logical data source".to_owned(),
                )
            })?;

        let value = params.get("logical").ok_or_else(|| {
            invalid(
                "logical",
                "the request path names no logical data source".to_owned(),
            )
        })?;

        LogicalDataSourceName::try_new(value)
            .map(Self)
            .map_err(|error| invalid("logical", error.to_string()))
    }
}
