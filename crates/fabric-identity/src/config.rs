//! Configuration for how the tenant identity context is derived.

use crate::TrustedIssuer;

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

    /// The claim carrying granted scopes. Defaults to `scope`.
    ///
    /// Configurable for the same reason the others are: `scope` is the OAuth 2
    /// name, but providers in the wild emit `scp` (Entra ID) or `permissions`
    /// (Auth0). Getting this wrong fails *open-ended* rather than loudly — an
    /// unmatched claim yields an empty scope list, so every scope check simply
    /// returns false and the deployment sees blanket 403s with no indication
    /// that the claim name is the cause.
    ///
    /// Like roles, scopes are read for authorization only and can never affect
    /// tenant selection (§23).
    pub scope_claim: String,

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

    /// The issuers this deployment trusts, and the tenant each one names.
    ///
    /// **Required.** The default is empty and [`Self::validate`] refuses it, so
    /// every deployment must state the binding before the process will start.
    /// See [`TrustedIssuer`] for why the tenant comes from here rather than
    /// from the token, and for the rules this list has to satisfy.
    ///
    /// This is one of *two* configurations of the same fact: the gateway in
    /// front of this service carries the same issuer set as its JWT policy
    /// (ADR 0019 §1). The two can only fail closed against each other, but
    /// neither catches this list binding an issuer to the *wrong* tenant —
    /// the gateway has no opinion about tenants and this registry is the
    /// authority. Generate both from one tenant list; do not maintain them
    /// twice.
    pub trusted_issuers: Vec<TrustedIssuer>,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            tenant_claim: "tenant_id".to_owned(),
            subject_claim: "sub".to_owned(),
            roles_claim: "roles".to_owned(),
            scope_claim: "scope".to_owned(),
            reject_tenant_header: true,
            // Empty, and `validate` refuses empty. A default that trusted
            // something would be a default that decided a tenant boundary.
            trusted_issuers: Vec::new(),
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
    ///
    /// Also returns a message if the issuer registry is unusable — see
    /// [`TrustedIssuer::validate_registry`], which is why a deployment that has
    /// not stated its tenant binding refuses to start rather than refusing
    /// every request.
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("tenant_claim", &self.tenant_claim),
            ("subject_claim", &self.subject_claim),
            ("roles_claim", &self.roles_claim),
            ("scope_claim", &self.scope_claim),
        ] {
            if value.trim().is_empty() {
                return Err(format!("identity.{field} must not be empty"));
            }
        }

        TrustedIssuer::validate_registry(&self.trusted_issuers)
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::TenantId;

    use super::*;

    /// A configuration that is valid apart from whatever a test breaks.
    ///
    /// The default registry is empty and refused, so every test about a claim
    /// name has to start from a configuration that would otherwise start.
    fn registered() -> IdentityConfig {
        IdentityConfig {
            trusted_issuers: vec![TrustedIssuer::new(
                "https://id.example.com/realms/acme",
                TenantId::try_new("acme").unwrap(),
            )],
            ..IdentityConfig::default()
        }
    }

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
            ..registered()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_scope_claim_defaults_to_the_oauth_name() {
        assert_eq!(IdentityConfig::default().scope_claim, "scope");
    }

    #[test]
    fn an_empty_scope_claim_name_is_rejected_at_startup() {
        // Without this, an empty name would match nothing, every scope check
        // would return false, and the deployment would see blanket 403s.
        let config = IdentityConfig {
            scope_claim: String::new(),
            ..registered()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn an_omitted_scope_claim_still_deserialises_alongside_deny_unknown_fields() {
        // `deny_unknown_fields` and container-level `default` have to keep
        // coexisting: existing config files name no scope_claim at all, and an
        // unknown key must still be an error rather than a silent typo.
        let config: IdentityConfig = serde_json::from_str(r#"{"tenant_claim":"tenant_id"}"#).unwrap();
        assert_eq!(config.scope_claim, "scope");

        assert!(serde_json::from_str::<IdentityConfig>(r#"{"scope_clam":"scp"}"#).is_err());
    }

    #[test]
    fn a_provider_specific_scope_claim_is_accepted() {
        let config: IdentityConfig = serde_json::from_str(r#"{"scope_claim":"scp"}"#).unwrap();

        assert_eq!(config.scope_claim, "scp");
        assert!(IdentityConfig {
            scope_claim: config.scope_claim,
            ..registered()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn a_runtime_with_no_trusted_issuers_refuses_to_start() {
        // Reached from `build_identity`, which is step 1 of the application
        // graph — so this is a process that does not start, never a request
        // that fails. ADR 0019 §2.
        let error = IdentityConfig::default().validate().unwrap_err();

        assert!(error.contains("identity.trusted_issuers"));
    }

    #[test]
    fn two_registrations_for_one_issuer_are_refused_at_startup() {
        // Which one won would depend on ordering, and the two name different
        // tenants — so the answer would be a coin toss about a tenant
        // boundary.
        let config = IdentityConfig {
            trusted_issuers: vec![
                TrustedIssuer::new(
                    "https://id.example.com/realms/acme",
                    TenantId::try_new("acme").unwrap(),
                ),
                TrustedIssuer::new(
                    "https://id.example.com/realms/acme",
                    TenantId::try_new("globex").unwrap(),
                ),
            ],
            ..IdentityConfig::default()
        };

        assert!(config
            .validate()
            .unwrap_err()
            .contains("https://id.example.com/realms/acme"));
    }

    #[test]
    fn a_blank_issuer_is_refused_as_an_unrendered_template() {
        // It matches nothing a provider emits — except a token that carries
        // `"iss": ""`, which would then be handed this entry's tenant.
        let config = IdentityConfig {
            trusted_issuers: vec![TrustedIssuer::new("  ", TenantId::try_new("acme").unwrap())],
            ..IdentityConfig::default()
        };

        assert!(config.validate().unwrap_err().contains("blank issuer"));
    }

    #[test]
    fn a_registry_deserialises_from_the_configuration_file() {
        let config: IdentityConfig = serde_json::from_str(
            r#"{"trusted_issuers":[{"issuer":"https://id.example.com/realms/acme","tenant":"acme"}]}"#,
        )
        .unwrap();

        assert!(config.validate().is_ok());
        assert_eq!(config.trusted_issuers.len(), 1);
    }
}
