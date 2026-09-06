//! Building requests and reading responses against the composed router.
//!
//! Deliberately duplicated from
//! `fabric-runtime-publication/tests/support/requests.rs` rather than
//! shared -- this crate must not depend on that one at all (`src/lib.rs`),
//! and the two files are short enough that a considered copy costs less
//! than a dependency edge that would trip an architecture check.

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::Request;
use serde_json::Value;

/// Encodes claims into an unsigned bearer token -- the trusted-ingress
/// posture every Data API composed test drives through, since signature
/// verification is the edge's job, not this one's
/// (`fabric_identity::TrustedIngressReader`).
fn token_for(claims: &Value) -> String {
    let Value::Object(object) = claims else {
        panic!("claims must be an object");
    };

    encode_unsigned_token(object)
}

/// Every test in this suite writes resource-relative paths (`/articles`, not
/// `/v1/data/articles`), so a version bump changes one function, not every
/// call site.
fn versioned(uri: &str) -> String {
    format!("{API_PREFIX}{uri}")
}

/// The claims a bearer token for `tenant` carries. The canonical `tenant_id`
/// claim is the only thing that ever selects a tenant.
pub fn claims_for(tenant: &str) -> Value {
    serde_json::json!({ "tenant_id": tenant })
}

/// A GET request carrying a bearer token and no body.
pub fn get(uri: &str, claims: &Value) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .body(Body::empty())
        .unwrap()
}

/// A POST request carrying a bearer token and a JSON body.
pub fn post(uri: &str, claims: &Value, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A GET request carrying a bearer token *and* a caller-supplied
/// `X-Tenant-Id` header -- the selection path the identity resolver refuses
/// outright regardless of what it names (`fabric_identity::resolver`).
pub fn get_with_tenant_header(uri: &str, claims: &Value, header_tenant: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .header("x-tenant-id", header_tenant)
        .body(Body::empty())
        .unwrap()
}

/// Reads a response body as JSON, or `Null` if it is not JSON.
pub async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// Reads a response body as raw text, for the "no response names X"
/// assertions, which need to search the literal bytes rather than a parsed
/// value.
pub async fn body_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    String::from_utf8_lossy(&bytes).into_owned()
}
