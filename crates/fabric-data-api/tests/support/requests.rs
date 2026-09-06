//! Building requests and reading responses.

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::Request;
use serde_json::Value;

use super::fixtures::issuer_naming;

/// Encodes claims into an unsigned bearer token, supplying a registered `iss`
/// when the caller did not name one.
///
/// **This is the choke point for every suite in this crate.** All fifteen build
/// their tokens here, so the issuer that ADR 0019 §2 makes the tenant binding
/// hinge on is added in one function rather than in fifteen files. A test that
/// cares which issuer minted the token — the cross-tenant and unregistered-issuer
/// cases — passes `iss` itself, and that value is used unchanged.
///
/// The default is derived from the token's own `tenant_id`, because the registry
/// registers one issuer per tenant: see `fixtures::issuer_naming`.
fn token_for(claims: Value) -> String {
    let Value::Object(mut object) = claims else {
        panic!("claims must be an object");
    };

    if !object.contains_key("iss") {
        let issuer = issuer_naming(object.get("tenant_id").and_then(Value::as_str));
        object.insert("iss".to_owned(), Value::String(issuer));
    }

    encode_unsigned_token(&object)
}

/// Every test in this suite writes resource-relative paths (`/customers`,
/// not `/v1/data/customers`) so a version bump changes one function, not
/// every call site.
fn versioned(uri: &str) -> String {
    format!("{API_PREFIX}{uri}")
}

/// A request carrying a bearer token and no body.
pub fn request(method: &str, uri: &str, claims: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .body(Body::empty())
        .unwrap()
}

/// A request carrying a bearer token and a JSON body.
pub fn json_request(method: &str, uri: &str, claims: Value, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Reads a response body as JSON, or `Null` if it is not JSON.
pub async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}
