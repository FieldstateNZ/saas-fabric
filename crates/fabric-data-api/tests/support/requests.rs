//! Building requests and reading responses.

use axum::body::Body;
use fabric_identity::encode_unsigned_token;
use http::Request;
use serde_json::Value;

/// Encodes claims into an unsigned bearer token.
fn token_for(claims: Value) -> String {
    let Value::Object(object) = claims else {
        panic!("claims must be an object");
    };

    encode_unsigned_token(&object)
}

/// A request carrying a bearer token and no body.
pub fn request(method: &str, uri: &str, claims: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .body(Body::empty())
        .unwrap()
}

/// A request carrying a bearer token and a JSON body.
pub fn json_request(method: &str, uri: &str, claims: Value, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
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
