//! The issuer names the tenant, and the claim is only allowed to agree.

use fabric_core::TenantId;

use crate::{logging, IdentityConfig, IdentityError, TokenClaims, TrustedIssuer};

/// The registered JWT claim carrying the issuer (RFC 7519 §4.1.1).
///
/// Not configurable, unlike the tenant, subject, roles and scope claims. Those
/// are deployment vocabulary; `iss` is the standard's own name and the same
/// string the gateway matched its allow-list against. A deployment that could
/// rename it could point the tenant binding at a claim the edge never checked.
const ISSUER_CLAIM: &str = "iss";

/// Decides the tenant for a token, from the registry and never from the claim.
///
/// Steps 3 to 6 of ADR 0019 §2, in order:
///
/// 3. read `iss` from the parsed claims;
/// 4. look it up in [`IdentityConfig::trusted_issuers`] — an issuer with no
///    registration is refused;
/// 5. take the tenant from the registration;
/// 6. read the configured tenant claim, which is required, and refuse a value
///    that does not equal the registration's tenant.
///
/// Step 4 is the substantive one and step 6 is what makes it safe to keep the
/// claim at all: this process verifies nothing, so a disagreement is the only
/// evidence it will ever get that the edge and this registry have diverged.
///
/// The returned tenant is **the registration's**, cloned — structurally, not
/// merely by test. Every path through this function ends either in an early
/// `Err` or in the `Ok(registration.tenant().clone())` below; there is no
/// third path that reads the claim instead. A test asserting "the registered
/// tenant is what gets used, even when the claim spells it the same" cannot
/// fail, because the claim is never in the return statement to begin with —
/// reading it back off the claim after checking equality would be the same
/// value today and the wrong value the moment the comparison loosened.
///
/// # Errors
///
/// [`IdentityError::MissingIssuerClaim`], [`IdentityError::UnregisteredIssuer`],
/// [`IdentityError::MissingTenantClaim`], [`IdentityError::InvalidTenantClaim`]
/// or [`IdentityError::TenantClaimDisagreesWithIssuer`]. All are `401`, and none
/// names a value the token carried.
pub(super) fn bind(config: &IdentityConfig, claims: &TokenClaims) -> Result<TenantId, IdentityError> {
    let registration = registration_for(config, claims)?;
    let claimed = claimed_tenant(config, claims)?;

    if claimed != *registration.tenant() {
        logging::tenant_claim_disagrees_with_issuer(&config.tenant_claim);
        return Err(IdentityError::TenantClaimDisagreesWithIssuer {
            claim: config.tenant_claim.clone(),
        });
    }

    Ok(registration.tenant().clone())
}

/// Steps 3 and 4: the registration this token's issuer selects.
fn registration_for<'a>(
    config: &'a IdentityConfig,
    claims: &TokenClaims,
) -> Result<&'a TrustedIssuer, IdentityError> {
    let issuer = claims.string(ISSUER_CLAIM).ok_or_else(|| {
        logging::issuer_claim_missing(ISSUER_CLAIM);
        IdentityError::MissingIssuerClaim
    })?;

    TrustedIssuer::find(&config.trusted_issuers, issuer).ok_or_else(|| {
        logging::issuer_unregistered(issuer, config.trusted_issuers.len());
        IdentityError::UnregisteredIssuer
    })
}

/// Step 6's first half: the tenant the token claims to be, parsed.
///
/// Parsed before it is compared, so a claim that is not an identifier is
/// reported as one rather than as a disagreement — the two are different
/// operator problems, and `InvalidTenantClaim` is the more useful of them.
fn claimed_tenant(config: &IdentityConfig, claims: &TokenClaims) -> Result<TenantId, IdentityError> {
    let tenant_claim = config.tenant_claim.as_str();

    let raw = claims.string(tenant_claim).ok_or_else(|| {
        logging::tenant_claim_missing(tenant_claim);
        IdentityError::MissingTenantClaim {
            claim: tenant_claim.to_owned(),
        }
    })?;

    TenantId::try_new(raw).map_err(|error| {
        logging::tenant_claim_invalid(tenant_claim, &error);
        IdentityError::InvalidTenantClaim {
            claim: tenant_claim.to_owned(),
        }
    })
}
