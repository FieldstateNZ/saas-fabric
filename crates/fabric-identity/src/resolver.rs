//! Turns request headers into a tenant identity context, or rejects them.

use std::sync::Arc;

use fabric_core::TenantId;
use http::HeaderMap;

use crate::bearer::extract_bearer;
use crate::logging;
use crate::{IdentityConfig, IdentityError, TenantIdentity, TokenReader};

/// Derives the tenant identity context for a request.
///
/// This is the single place in the platform where a tenant is decided. The
/// order of operations is deliberate and worth reading:
///
/// 1. Reject a banned tenant header, if configured to (§11).
/// 2. Extract the bearer token.
/// 3. Read its claims, via the configured [`TokenReader`].
/// 4. Take the tenant from the configured claim — and nowhere else (§10).
///
/// Step 1 comes first so that a caller attempting header-based tenant selection
/// is told plainly, rather than being handed a successful response for a
/// different tenant than the one it asked for.
pub struct IdentityResolver {
    config: IdentityConfig,
    reader: Arc<dyn TokenReader>,
}

impl IdentityResolver {
    /// Builds a resolver. Called from
    /// [`build_identity`](crate::build_identity).
    #[must_use]
    pub fn new(config: IdentityConfig, reader: Arc<dyn TokenReader>) -> Self {
        Self { config, reader }
    }

    /// Resolves the tenant identity context from request headers.
    ///
    /// # Errors
    ///
    /// Any [`IdentityError`]. Every one of them is a rejection — there is no
    /// partial success and no default tenant (§28).
    pub fn resolve(&self, headers: &HeaderMap) -> Result<TenantIdentity, IdentityError> {
        self.reject_tenant_header(headers)?;

        let token = extract_bearer(headers)?;
        let claims = self.reader.read(token)?;

        let tenant_claim = self.config.tenant_claim.as_str();

        let raw_tenant = claims.string(tenant_claim).ok_or_else(|| {
            logging::tenant_claim_missing(tenant_claim);
            IdentityError::MissingTenantClaim {
                claim: tenant_claim.to_owned(),
            }
        })?;

        let tenant = TenantId::try_new(raw_tenant).map_err(|error| {
            logging::tenant_claim_invalid(tenant_claim, &error);
            IdentityError::InvalidTenantClaim {
                claim: tenant_claim.to_owned(),
            }
        })?;

        let subject = claims
            .string(&self.config.subject_claim)
            .unwrap_or_default()
            .to_owned();
        let roles = claims.string_list(&self.config.roles_claim);
        let scopes = claims.string_list("scope");

        Ok(TenantIdentity::new(tenant, subject, roles, scopes))
    }

    /// Enforces §11's ban on caller-supplied tenant selection.
    ///
    /// Note that the header is never *read* as a tenant source regardless of
    /// this setting — there is no code path that does. This only decides
    /// whether its presence is an error or is ignored.
    fn reject_tenant_header(&self, headers: &HeaderMap) -> Result<(), IdentityError> {
        if !headers.contains_key(IdentityConfig::BANNED_TENANT_HEADER) {
            return Ok(());
        }

        if self.config.reject_tenant_header {
            logging::tenant_header_rejected(IdentityConfig::BANNED_TENANT_HEADER);
            return Err(IdentityError::TenantHeaderPresent {
                header: IdentityConfig::BANNED_TENANT_HEADER,
            });
        }

        logging::tenant_header_ignored(IdentityConfig::BANNED_TENANT_HEADER);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use fabric_core::Clock;
    use serde_json::json;

    use super::*;
    use crate::readers::{encode_unsigned_token, TrustedIngressReader};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            1_000
        }
    }

    fn resolver() -> IdentityResolver {
        IdentityResolver::new(
            IdentityConfig::default(),
            Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
        )
    }

    fn headers_for(claims: serde_json::Value) -> HeaderMap {
        let serde_json::Value::Object(object) = claims else {
            panic!("claims must be a JSON object");
        };

        let token = encode_unsigned_token(&object);
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
        let headers = headers_for(json!({"tenant_id": "Acme Corp"}));

        assert_eq!(
            resolver().resolve(&headers).unwrap_err(),
            IdentityError::InvalidTenantClaim {
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
        let config = IdentityConfig {
            reject_tenant_header: false,
            ..IdentityConfig::default()
        };
        let resolver =
            IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))));

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
    fn a_configurable_claim_name_is_honoured() {
        let config = IdentityConfig {
            tenant_claim: "https://example.com/tenant".to_owned(),
            ..IdentityConfig::default()
        };
        let resolver =
            IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))));

        let headers = headers_for(json!({"https://example.com/tenant": "acme"}));
        assert_eq!(resolver.resolve(&headers).unwrap().tenant().as_str(), "acme");
    }
}
