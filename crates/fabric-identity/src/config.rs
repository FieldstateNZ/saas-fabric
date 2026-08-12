//! Configuration for how the tenant identity context is derived.

/// How this deployment reads identity out of a bearer token.
///
/// Claim names are configurable because §10 permits it, but the defaults match
/// the specification's canonical names and **should be standardised across
/// platform services** — a deployment where the Data API and the Configuration
/// API disagree about which claim carries the tenant is a deployment with two
/// tenant contexts, which is precisely what §11 forbids.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct IdentityConfig {
    /// The claim carrying the canonical tenant identity. Defaults to
    /// `tenant_id` (§10).
    pub tenant_claim: String,

    /// The claim carrying the authenticated principal. Defaults to `sub`.
    pub subject_claim: String,

    /// The claim carrying role names. Defaults to `roles`.
    ///
    /// Roles are read for *authorization* only. Per §23 they can never affect
    /// which tenant is selected.
    pub roles_claim: String,

    /// Whether to reject a request that carries a tenant-selection header.
    ///
    /// Defaults to `true`. The tenant is never read from a header either way —
    /// that is not configurable, because §11 makes it a hard requirement. This
    /// switch only controls whether an attempt to use one is met with a 400 or
    /// silently ignored.
    ///
    /// Rejecting is the default because silence is the more dangerous
    /// behaviour: a caller sending `X-Tenant-Id: acme` and getting back
    /// `globex` data has been told nothing is wrong.
    pub reject_tenant_header: bool,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            tenant_claim: "tenant_id".to_owned(),
            subject_claim: "sub".to_owned(),
            roles_claim: "roles".to_owned(),
            reject_tenant_header: true,
        }
    }
}

impl IdentityConfig {
    /// The header that §11 forbids as a tenant-selection mechanism.
    pub const BANNED_TENANT_HEADER: &'static str = "x-tenant-id";

    /// Checks the configuration before any service is built.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending field if a claim name is empty.
    /// An empty claim name would silently never match, which would make every
    /// request fail closed with no obvious cause.
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("tenant_claim", &self.tenant_claim),
            ("subject_claim", &self.subject_claim),
            ("roles_claim", &self.roles_claim),
        ] {
            if value.trim().is_empty() {
                return Err(format!("identity.{field} must not be empty"));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_specification_canonical_claim() {
        assert_eq!(IdentityConfig::default().tenant_claim, "tenant_id");
    }

    #[test]
    fn rejecting_the_tenant_header_is_the_default_posture() {
        assert!(IdentityConfig::default().reject_tenant_header);
    }

    #[test]
    fn an_empty_claim_name_is_rejected_at_startup() {
        let config = IdentityConfig {
            tenant_claim: "  ".to_owned(),
            ..IdentityConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
