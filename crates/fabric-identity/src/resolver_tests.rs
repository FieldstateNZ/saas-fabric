//! Deriving a tenant identity, and the ways it must refuse.

use std::sync::Arc;
use std::time::Instant;

use fabric_core::{Clock, SystemClock, TenantId};
use http::HeaderMap;
use serde_json::{json, Map, Value};

use crate::readers::{encode_unsigned_token, TrustedIngressReader};
use crate::resolver::IdentityResolver;
use crate::{IdentityConfig, IdentityError, TrustedIssuer};

/// The issuer registered to `acme`, and the one `headers_for` supplies when a
/// test does not care which issuer minted the token.
const ACME_ISSUER: &str = "https://id.example.com/realms/acme";

/// The issuer registered to `globex`. Present so a test can build the case
/// ADR 0019 §2 is about: a token whose issuer and whose claim disagree.
const GLOBEX_ISSUER: &str = "https://id.example.com/realms/globex";

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

/// Two tenants, two issuers — the shape a runtime serving more than one tenant
/// actually has.
fn registry() -> Vec<TrustedIssuer> {
    vec![
        TrustedIssuer::new(ACME_ISSUER, TenantId::try_new("acme").unwrap()),
        TrustedIssuer::new(GLOBEX_ISSUER, TenantId::try_new("globex").unwrap()),
    ]
}

fn config() -> IdentityConfig {
    IdentityConfig {
        trusted_issuers: registry(),
        ..IdentityConfig::default()
    }
}

fn resolver_with(config: IdentityConfig) -> IdentityResolver {
    IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))))
}

fn resolver() -> IdentityResolver {
    resolver_with(config())
}

/// Turns a claim set into an `Authorization` header, supplying `ACME_ISSUER`
/// when the test did not name an issuer of its own.
///
/// Defaulting rather than requiring every case to spell out an `iss` keeps the
/// tests about the thing each one is testing. A test that cares which issuer
/// minted the token says so, and its value is used unchanged.
fn headers_for(claims: Value) -> HeaderMap {
    let Value::Object(mut object) = claims else {
        panic!("claims must be a JSON object");
    };

    object
        .entry("iss".to_owned())
        .or_insert_with(|| Value::String(ACME_ISSUER.to_owned()));

    headers_holding(&encode_unsigned_token(&object))
}

fn headers_holding(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

#[test]
fn resolves_the_tenant_from_the_canonical_claim() {
    let headers = headers_for(json!({"sub": "user-123", "tenant_id": "acme", "roles": ["user"]}));
    let identity = resolver().resolve(&headers).unwrap();

    assert_eq!(identity.tenant().as_str(), "acme");
    assert_eq!(identity.subject(), "user-123");
    assert!(identity.has_role("user"));
}

#[test]
fn a_missing_tenant_claim_is_rejected_rather_than_defaulted() {
    // Still required after ADR 0019 §2, even though the issuer already names
    // the tenant: a token without the canonical claim comes from a realm that
    // was not configured the way §10 says a realm is configured.
    let headers = headers_for(json!({"sub": "user-123"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::MissingTenantClaim {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_tenant_claim_that_is_not_a_valid_identifier_is_rejected() {
    // Parsed before it is compared, so this reports the shape of the value and
    // not "it disagreed" — two different operator problems.
    let headers = headers_for(json!({"tenant_id": "Acme Corp"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::InvalidTenantClaim {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_token_from_an_unregistered_issuer_is_refused() {
    // Genuinely signed, perfectly well-formed, and naming a tenant this
    // runtime serves. It is refused because nothing here binds that issuer to
    // that tenant, and the registry is the authority (ADR 0019 §1).
    let headers = headers_for(json!({"iss": "https://id.example.com/realms/evil", "tenant_id": "acme"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::UnregisteredIssuer
    );
}

#[test]
fn a_token_with_no_issuer_is_refused_rather_than_treated_as_unregistered() {
    // ADR 0002 records the other shape of this hole: a token that simply
    // omitted `iss` sailed past an issuer allowlist, which made the control
    // silently do nothing. Absent is refused here, not waved through.
    let claims = Map::new();
    let headers = headers_holding(&encode_unsigned_token(&claims));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::MissingIssuerClaim
    );
}

#[test]
fn a_non_string_issuer_claim_reads_as_absent_rather_than_being_coerced() {
    // Same reasoning as the tenant claim: `iss: 42` is a misconfigured
    // provider, and stringifying it would hide that.
    let headers = headers_for(json!({"iss": 42, "tenant_id": "acme"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::MissingIssuerClaim
    );
}

#[test]
fn a_tenant_claim_that_disagrees_with_its_issuer_is_refused() {
    // The cross-tenant case. Not a request to disambiguate — a request to
    // pick, and picking is the bug. The tenant is `acme` or there is no tenant.
    let headers = headers_for(json!({"iss": ACME_ISSUER, "tenant_id": "globex"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::TenantClaimDisagreesWithIssuer {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_tenant_that_agrees_with_its_issuer_is_the_tenant_that_is_used() {
    // Both directions, so this cannot be read as "whichever the claim said".
    let acme = headers_for(json!({"iss": ACME_ISSUER, "tenant_id": "acme"}));
    let globex = headers_for(json!({"iss": GLOBEX_ISSUER, "tenant_id": "globex"}));

    assert_eq!(resolver().resolve(&acme).unwrap().tenant().as_str(), "acme");
    assert_eq!(resolver().resolve(&globex).unwrap().tenant().as_str(), "globex");
}

#[test]
fn the_registered_tenant_is_what_is_used_even_when_the_claim_spells_it_the_same() {
    // The value returned is the registration's, cloned. Reading it back off
    // the claim would be the same string today and the wrong one the moment
    // the comparison loosened.
    let registration = TenantId::try_new("acme").unwrap();
    let headers = headers_for(json!({"iss": ACME_ISSUER, "tenant_id": "acme"}));

    assert_eq!(*resolver().resolve(&headers).unwrap().tenant(), registration);
}

#[test]
fn a_claim_projection_header_changes_nothing_about_the_tenant() {
    // The edge strips these; the runtime is written never to notice them. Both
    // requests must resolve the same tenant, or a projected claim would be a
    // second identity source arriving on the trusted side of the boundary.
    let claims = json!({"iss": ACME_ISSUER, "tenant_id": "acme"});

    let plain = resolver().resolve(&headers_for(claims.clone())).unwrap();

    let mut projected = headers_for(claims);
    projected.insert("x-jwt-claim-tenant-id", "globex".parse().unwrap());
    projected.insert("x-forwarded-user", "somebody@globex.example".parse().unwrap());

    assert_eq!(
        resolver().resolve(&projected).unwrap().tenant(),
        plain.tenant(),
        "a projected claim header must be inert"
    );
}

#[test]
fn the_client_identity_claim_is_not_an_identity_source_on_the_data_api_path() {
    // Scoped in the name on purpose: ADR 0010's operator plane *does* gate on
    // `azp`, on a different route, in a different realm, for a different
    // question. This says only that nothing reads it here.
    let headers = headers_for(json!({
        "iss": ACME_ISSUER,
        "tenant_id": "acme",
        "azp": "globex-mobile",
        "client_id": "globex-mobile",
    }));

    assert_eq!(resolver().resolve(&headers).unwrap().tenant().as_str(), "acme");
}

#[test]
fn the_defence_in_depth_posture_binds_the_tenant_through_the_same_registry() {
    // The binding belongs to the resolver, not to a reader, so a deployment
    // cannot acquire signature verification and lose the tenant boundary. This
    // drives the real `ValidatingReader` with only its signature check
    // switched off — see `posture_parity_tests` for why that fixture exists.
    const SECRET: &[u8] = b"resolver-binding-fixture";

    let at = SystemClock.now_unix_seconds();
    let claims = json!({"iss": ACME_ISSUER, "tenant_id": "globex", "exp": at + 3_600});

    let reader = crate::readers::validating::tests::insecure_reader(SECRET, Arc::new(FixedClock));
    let resolver = IdentityResolver::new(config(), Arc::new(reader));

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(SECRET),
    )
    .unwrap();

    assert_eq!(
        resolver.resolve(&headers_holding(&token)).unwrap_err(),
        IdentityError::TenantClaimDisagreesWithIssuer {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_request_carrying_the_banned_tenant_header_is_rejected() {
    let mut headers = headers_for(json!({"tenant_id": "acme"}));
    headers.insert("x-tenant-id", "globex".parse().unwrap());

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::TenantHeaderPresent {
            header: "x-tenant-id"
        }
    );
}

#[test]
fn the_header_never_overrides_the_token_even_when_it_is_only_ignored() {
    // This is the ambiguous state §11 exists to make impossible: token says
    // acme, header says globex. With rejection switched off the header must
    // still have no effect whatsoever.
    let resolver = resolver_with(IdentityConfig {
        reject_tenant_header: false,
        ..config()
    });

    let mut headers = headers_for(json!({"tenant_id": "acme"}));
    headers.insert("x-tenant-id", "globex".parse().unwrap());

    assert_eq!(resolver.resolve(&headers).unwrap().tenant().as_str(), "acme");
}

#[test]
fn a_request_with_no_authorization_header_is_rejected() {
    assert_eq!(
        resolver().resolve(&HeaderMap::new()).unwrap_err(),
        IdentityError::MissingAuthorization
    );
}

#[test]
fn scopes_are_read_from_the_default_oauth_claim() {
    let headers = headers_for(json!({"tenant_id": "acme", "scope": "read write"}));
    let identity = resolver().resolve(&headers).unwrap();

    assert!(identity.has_scope("read"));
    assert!(identity.has_scope("write"));
}

#[test]
fn a_configurable_scope_claim_name_is_honoured() {
    // Entra ID emits `scp`; Auth0 emits `permissions`. Before this was
    // configurable such a deployment silently resolved zero scopes, which
    // surfaces as blanket 403s rather than as an error.
    let resolver = resolver_with(IdentityConfig {
        scope_claim: "scp".to_owned(),
        ..config()
    });

    let headers = headers_for(json!({"tenant_id": "acme", "scp": ["read", "write"]}));
    let identity = resolver.resolve(&headers).unwrap();

    assert!(identity.has_scope("read"));
    assert!(identity.has_scope("write"));
}

#[test]
fn the_default_scope_claim_is_ignored_once_another_name_is_configured() {
    // A provider that emits both must not have the unconfigured claim leak in
    // — otherwise the configured name is an addition rather than a choice.
    let resolver = resolver_with(IdentityConfig {
        scope_claim: "scp".to_owned(),
        ..config()
    });

    let headers = headers_for(json!({"tenant_id": "acme", "scp": ["read"], "scope": "admin"}));
    let identity = resolver.resolve(&headers).unwrap();

    assert!(identity.has_scope("read"));
    assert!(!identity.has_scope("admin"));
}

#[test]
fn a_token_that_is_not_yet_valid_is_refused_before_a_tenant_is_derived() {
    // FixedClock sits at 1_000, so this token is valid well beyond the leeway.
    let headers = headers_for(json!({"tenant_id": "acme", "nbf": 9_000}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::TokenNotYetValid
    );
}

#[test]
fn a_configurable_claim_name_is_honoured() {
    let resolver = resolver_with(IdentityConfig {
        tenant_claim: "https://example.com/tenant".to_owned(),
        ..config()
    });

    let headers = headers_for(json!({"https://example.com/tenant": "acme"}));
    assert_eq!(resolver.resolve(&headers).unwrap().tenant().as_str(), "acme");
}
