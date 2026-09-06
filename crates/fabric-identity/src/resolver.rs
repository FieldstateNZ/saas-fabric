//! Turns request headers into a tenant identity context, or rejects them.

mod tenant_binding;

use std::sync::Arc;

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
/// 4. Bind the token's `iss` to a tenant through
///    [`IdentityConfig::trusted_issuers`], requiring the tenant claim to agree
///    (ADR 0019 §2). `tenant_binding` holds that step and explains it.
///
/// Step 1 comes first so that a caller attempting header-based tenant selection
/// is told plainly, rather than being handed a successful response for a
/// different tenant than the one it asked for.
///
/// Step 4 sits here rather than in a [`TokenReader`] on purpose: it is a
/// property of the resolver, so the canonical and defence-in-depth postures
/// reach it by the same path and no deployment can run without it.
pub struct IdentityResolver {
    config: IdentityConfig,
    reader: Arc<dyn TokenReader>,
}

impl IdentityResolver {
    /// Builds a resolver, refusing an invalid configuration.
    ///
    /// A resolver cannot exist without a validated issuer registry — that is
    /// a property of the type, not a step a caller can skip by constructing
    /// one directly instead of going through
    /// [`build_identity`](crate::build_identity). Every downstream read of
    /// `self.config` therefore trusts what construction already proved,
    /// rather than re-checking it.
    ///
    /// # Errors
    ///
    /// Returns a message from [`IdentityConfig::validate`] if the
    /// configuration is invalid.
    pub fn new(config: IdentityConfig, reader: Arc<dyn TokenReader>) -> Result<Self, String> {
        config.validate()?;

        Ok(Self { config, reader })
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

        let tenant = tenant_binding::bind(&self.config, &claims)?;

        let subject = claims
            .string(&self.config.subject_claim)
            .unwrap_or_default()
            .to_owned();
        let roles = claims.string_list(&self.config.roles_claim);
        let scopes = claims.string_list(&self.config.scope_claim);

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
