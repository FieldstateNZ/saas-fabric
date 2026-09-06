//! Building requests and reading responses, the same way
//! `fabric-data-api`'s own composed tests do.

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::Request;
use serde_json::Value;

/// Encodes claims into an unsigned bearer token -- the trusted-ingress
/// posture every Data API composed test drives through, since signature
/// verification is the edge's job, not this one's (see
/// `fabric_identity::TrustedIngressReader`).
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

/// The tenants this suite drives through one resolver.
const REGISTERED_TENANTS: [&str; 2] = ["acme", "globex"];

/// The issuer registered to `tenant`.
fn issuer_for(tenant: &str) -> String {
    format!("https://identity.test.invalid/realms/{tenant}")
}

/// The identity registry these tests run against: one issuer per tenant.
///
/// A registration binds one issuer to one tenant, and this suite drives two
/// tenants through a single resolver, so a shared issuer could only name one
/// of them.
pub fn trusted_issuers() -> Vec<fabric_identity::TrustedIssuer> {
    REGISTERED_TENANTS
        .iter()
        .map(|tenant| {
            fabric_identity::TrustedIssuer::new(
                issuer_for(tenant),
                fabric_core::TenantId::try_new(tenant).unwrap(),
            )
        })
        .collect()
}

/// The claims a bearer token for `tenant` carries.
///
/// The tenant comes from the issuer's registration (ADR 0019 §2), and the
/// canonical `tenant_id` claim is required to agree with it — so both are
/// written here, and nothing else ever selects a tenant (ADR 0018, "The
/// Synthesis Cloud record-isolation seam", item 4).
pub fn claims_for(tenant: &str) -> Value {
    serde_json::json!({ "iss": issuer_for(tenant), "tenant_id": tenant })
}

/// A request carrying a bearer token and no body.
pub fn request(method: &str, uri: &str, claims: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(versioned(uri))
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .body(Body::empty())
        .unwrap()
}

/// A request carrying a bearer token *and* a caller-supplied `X-Tenant-Id`
/// header -- the second selection path ADR 0018 says publication must not
/// introduce. Built separately from [`request`] because no other test needs
/// the header at all, and every other request in this suite should keep
/// proving that.
pub fn request_with_tenant_header(uri: &str, claims: &Value, header_tenant: &str) -> Request<Body> {
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
