//! One issuer this runtime trusts, and the tenant its tokens belong to.
//!
//! Over the 120-line guidance deliberately, and mostly in rustdoc: this is one
//! small type, its two accessors, the lookup that selects it, and the rules a
//! registry of them must satisfy. Every item here exists only because the type
//! does, and none would be tested or reused apart from it. What the length
//! actually buys is the argument — *why* the tenant comes from a registration
//! rather than a claim, and why each refusal is a startup failure — which is
//! the part a reader has to have and the part a split would scatter.

use std::collections::BTreeSet;

use fabric_core::TenantId;

/// A registration binding one exact issuer string to one tenant.
///
/// # Why the tenant is registry-derived rather than claimed
///
/// A `tenant_id` claim is a statement an identity provider made. A provider
/// that mints it from a user-editable attribute hands every one of its users a
/// cross-tenant read, and nothing in a token can tell the runtime that has
/// happened. The issuer is different: on this path it is the one thing about a
/// token the edge has already proved, so binding it here makes the tenant a
/// fact this deployment configured rather than one the token asserted.
///
/// ADR 0016 already settles this one route over, at the authorization front
/// door: the tenant, the realm identity and the store come exclusively from the
/// registration selected by the verified `iss`, and a token carrying a
/// plausible `tenant` claim is not wrong to hold it — it is simply never read.
/// ADR 0019 §2 makes the Data API path say the same thing, with one difference:
/// here the claim is *also* required to agree, because this process verifies
/// nothing itself and a disagreement is the only signal it will ever get that
/// the edge and this registry have diverged.
///
/// # Why this is not `fabric-fga-auth`'s `IssuerRegistration`
///
/// That type carries a `jwks_uri`, an algorithm allow-list, a store and a
/// pinned authorization model, because the front door verifies for itself. None
/// of those may be known here: §24 keeps the runtime plane independent of any
/// identity implementation, and this crate depends on `fabric-core` and nothing
/// else. One concept, two shapes, because the two hops need different facts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedIssuer {
    /// The value a token's `iss` must equal, exactly.
    issuer: String,

    /// The tenant every token from that issuer belongs to.
    tenant: TenantId,
}

impl TrustedIssuer {
    /// Registers `issuer` as naming `tenant`.
    #[must_use]
    pub fn new(issuer: impl Into<String>, tenant: TenantId) -> Self {
        Self {
            issuer: issuer.into(),
            tenant,
        }
    }

    /// The value a token's `iss` must equal.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The tenant this issuer's tokens belong to.
    #[must_use]
    pub const fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Finds the registration for an issuer, comparing the whole string.
    ///
    /// Exact match, never a prefix and never a pattern — an issuer that matches
    /// loosely is an issuer somebody else can look like. Under a prefix rule
    /// `https://id.example.com/realms/acme-evil` would select the registration
    /// written for `https://id.example.com/realms/acme`, and the tenant
    /// boundary would be whatever a realm administrator felt like naming their
    /// realm.
    #[must_use]
    pub fn find<'a>(registry: &'a [Self], issuer: &str) -> Option<&'a Self> {
        registry.iter().find(|registration| registration.issuer == issuer)
    }

    /// Refuses a registry that cannot decide a tenant boundary.
    ///
    /// Three states, each fatal at startup rather than per request:
    ///
    /// - **Empty.** ADR 0016 records why: an empty registry is the shape in
    ///   which a service quietly trusts nothing — or, in at least one real
    ///   implementation, quietly trusts *everything*. Here it would refuse
    ///   every token, which is safe and is still a deployment nobody meant.
    /// - **A blank issuer.** It matches no value a real identity provider
    ///   emits, so it is an unrendered template — and a token carrying
    ///   `"iss": ""` would match it and be handed that entry's tenant.
    /// - **The same issuer twice.** Which registration won would depend on
    ///   ordering, and the two may name different tenants.
    ///
    /// The messages name `identity.trusted_issuers` because that is the only
    /// place a registry is ever configured, and an error naming the type
    /// rather than the setting would leave the operator to find it.
    ///
    /// # Errors
    ///
    /// Returns a message describing the first problem found.
    pub fn validate_registry(registry: &[Self]) -> Result<(), String> {
        if registry.is_empty() {
            return Err(
                "identity.trusted_issuers is empty: a runtime that trusts no issuer cannot bind \
                        any token to a tenant. Add at least one [[identity.trusted_issuers]] entry \
                        naming an issuer and the tenant it serves"
                    .to_owned(),
            );
        }

        let mut seen = BTreeSet::new();

        for registration in registry {
            if registration.issuer.trim().is_empty() {
                return Err(
                    "identity.trusted_issuers has an entry with a blank issuer, which matches \
                            nothing an identity provider emits"
                        .to_owned(),
                );
            }

            if !seen.insert(registration.issuer.as_str()) {
                return Err(format!(
                    "identity.trusted_issuers registers {} more than once; which registration wins \
                     would depend on ordering, and the two may name different tenants",
                    registration.issuer
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Vec<TrustedIssuer> {
        vec![
            TrustedIssuer::new(
                "https://id.example.com/realms/acme",
                TenantId::try_new("acme").unwrap(),
            ),
            TrustedIssuer::new(
                "https://id.example.com/realms/globex",
                TenantId::try_new("globex").unwrap(),
            ),
        ]
    }

    #[test]
    fn a_registered_issuer_names_its_own_tenant() {
        let registry = registry();
        let found = TrustedIssuer::find(&registry, "https://id.example.com/realms/globex");

        assert_eq!(found.map(|entry| entry.tenant().as_str()), Some("globex"));
    }

    #[test]
    fn an_issuer_that_only_starts_the_same_way_is_not_the_registered_issuer() {
        // A prefix rule would hand this token `acme`. The registration is for
        // the whole string or it is for nothing.
        let registry = registry();
        let found = TrustedIssuer::find(&registry, "https://id.example.com/realms/acme-evil");

        assert!(found.is_none());
    }

    #[test]
    fn a_registration_deserialises_from_the_two_fields_it_has() {
        let registration: TrustedIssuer =
            serde_json::from_str(r#"{"issuer":"https://id.example.com/realms/acme","tenant":"acme"}"#)
                .unwrap();

        assert_eq!(registration.issuer(), "https://id.example.com/realms/acme");
        assert_eq!(registration.tenant().as_str(), "acme");
    }

    #[test]
    fn a_tenant_that_is_not_an_identifier_cannot_be_registered() {
        // The registry is configuration, so this is a startup failure rather
        // than a value that reaches a request.
        let result = serde_json::from_str::<TrustedIssuer>(
            r#"{"issuer":"https://id.example.com/realms/acme","tenant":"Acme Corp"}"#,
        );

        assert!(result.is_err());
    }
}
