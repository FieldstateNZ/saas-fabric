//! A JSON body extractor with a caller-configured maximum size.
//!
//! Axum's own body-size guard, `DefaultBodyLimit`, works by rejecting inside
//! the `Bytes`/`Json` extractors it patches, which produces a bare `413` in
//! axum's own body shape — not this crate's `{"error": {"code", "message"}}`
//! envelope, and not the `400` the rest of this crate's limits use (§28,
//! §29). [`BoundedJson`] reads the body the same protected way — a hard cap
//! passed to [`axum::body::to_bytes`], so a caller cannot force an unbounded
//! allocation by lying about `Content-Length` or streaming past it — but
//! turns an overflow into a [`DataApiError::BadRequest`], so every limit in
//! this crate, this one included, answers the same way.

use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use crate::{DataApiError, DataApiState};

/// A JSON request body, rejected before parsing if it exceeds
/// [`DataApiConfig::max_request_body_bytes`](crate::DataApiConfig).
#[derive(Debug)]
pub(crate) struct BoundedJson<T>(pub(crate) T);

impl<T> FromRequest<DataApiState> for BoundedJson<T>
where
    T: DeserializeOwned,
{
    type Rejection = DataApiError;

    async fn from_request(request: Request, state: &DataApiState) -> Result<Self, Self::Rejection> {
        let max = state.service.config().max_request_body_bytes;
        let limit = usize::try_from(max).unwrap_or(usize::MAX);

        let bytes = axum::body::to_bytes(request.into_body(), limit)
            .await
            .map_err(|_| {
                DataApiError::BadRequest(format!(
                    "the request body could not be read; it must not exceed {max} bytes"
                ))
            })?;

        let value = serde_json::from_slice(&bytes)
            .map_err(|error| DataApiError::BadRequest(format!("invalid JSON body: {error}")))?;

        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use fabric_connector::ConnectorRegistry;
    use fabric_identity::{build_identity, IdentityConfig, TrustedIngressReader};
    use fabric_tenant_runtime::{DataSourceRegistry, RuntimeResolver, TenantRegistry};
    use http::Request as HttpRequest;
    use std::sync::Arc;

    use super::*;
    use crate::{DataApiConfig, DataApiService, ResourceCatalog, ResourcePermissions};

    struct FixedClock;

    impl fabric_core::Clock for FixedClock {
        fn now(&self) -> std::time::Instant {
            std::time::Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            1_000
        }
    }

    fn state_with_limit(max_request_body_bytes: u32) -> DataApiState {
        let runtime = Arc::new(RuntimeResolver::new(
            Arc::new(TenantRegistry::new()),
            Arc::new(DataSourceRegistry::new()),
        ));
        let identity = build_identity(
            IdentityConfig::default(),
            Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
        )
        .unwrap();

        let config = DataApiConfig {
            max_request_body_bytes,
            ..DataApiConfig::default()
        };

        DataApiState {
            service: Arc::new(DataApiService::new(
                runtime,
                ConnectorRegistry::new(),
                ResourceCatalog::default(),
                ResourcePermissions::default(),
                config,
            )),
            identity,
        }
    }

    #[tokio::test]
    async fn a_body_within_the_limit_parses() {
        let state = state_with_limit(1024);
        let request = HttpRequest::builder()
            .body(Body::from(r#"{"name":"Alice"}"#))
            .unwrap();

        let BoundedJson(value) = BoundedJson::<serde_json::Value>::from_request(request, &state)
            .await
            .unwrap();

        assert_eq!(value["name"], "Alice");
    }

    #[tokio::test]
    async fn a_body_over_the_limit_is_rejected_before_parsing() {
        let state = state_with_limit(4);
        let request = HttpRequest::builder()
            .body(Body::from(r#"{"name":"Alice"}"#))
            .unwrap();

        let error = BoundedJson::<serde_json::Value>::from_request(request, &state)
            .await
            .unwrap_err();

        assert!(matches!(error, DataApiError::BadRequest(_)));
    }
}
