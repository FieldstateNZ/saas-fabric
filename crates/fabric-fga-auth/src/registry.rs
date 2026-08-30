//! The trusted issuer registry: the only thing that decides which tenant a
//! verified token belongs to.
//!
//! # It is checked once, at startup, and never again
//!
//! Every problem this module can find is fatal before anything is served. A
//! registry with no issuers authenticates nobody; a registry with a duplicated
//! issuer resolves to whichever registration a map happened to keep. Neither
//! is a condition to discover from a request, so [`Registry::build`] is the
//! only constructor and it returns a [`ConfigurationError`] instead.
//!
//! That is a deliberate reading of what other implementations get wrong: an
//! empty issuer list is exactly the shape in which one widely used
//! authorization service stops verifying signatures altogether, and it does it
//! silently.

mod registration;
#[cfg(test)]
mod registry_tests;

use std::collections::BTreeMap;

use crate::ConfigurationError;

pub use registration::IssuerRegistration;

/// Every issuer Fabric trusts, indexed by the exact `iss` that selects it.
#[derive(Debug, Clone)]
pub struct Registry {
    /// Registrations by issuer. Ordered so that a rendered diagnostic lists
    /// them the same way twice.
    issuers: BTreeMap<String, IssuerRegistration>,
}

impl Registry {
    /// Builds a registry, refusing anything that could not be trusted.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] for an empty registry, a duplicated
    /// issuer, or a registration that is not usable as written.
    pub fn build(
        registrations: impl IntoIterator<Item = IssuerRegistration>,
    ) -> Result<Self, ConfigurationError> {
        let mut issuers = BTreeMap::new();

        for registration in registrations {
            check(&registration)?;

            if issuers.contains_key(&registration.issuer) {
                return Err(ConfigurationError::DuplicateIssuer {
                    issuer: registration.issuer,
                });
            }

            issuers.insert(registration.issuer.clone(), registration);
        }

        if issuers.is_empty() {
            return Err(ConfigurationError::NoIssuers);
        }

        Ok(Self { issuers })
    }

    /// The registration for an issuer, if Fabric trusts it.
    ///
    /// Exact match. An issuer that matched a prefix or a pattern would be an
    /// issuer somebody else could look like.
    #[must_use]
    pub fn registration(&self, issuer: &str) -> Option<&IssuerRegistration> {
        self.issuers.get(issuer)
    }

    /// How many issuers are trusted. Never zero — see [`Registry::build`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.issuers.len()
    }

    /// Always `false`, and present because clippy asks for it beside `len`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Everything one registration must satisfy to be usable.
fn check(registration: &IssuerRegistration) -> Result<(), ConfigurationError> {
    let invalid = |detail: &str| ConfigurationError::InvalidRegistration {
        issuer: registration.issuer.clone(),
        detail: detail.to_owned(),
    };

    if registration.issuer.trim().is_empty() {
        return Err(invalid("issuer must not be empty"));
    }

    // The tenant becomes the realm half of every principal minted here, so it
    // takes the platform's realm rule. Checked at startup rather than when the
    // first token arrives, because a registration that cannot mint a principal
    // is broken whether or not anybody signs in.
    if fabric_core::SubjectId::from_verified(&registration.tenant, "probe").is_err() {
        return Err(invalid(
            "tenant is not a valid realm identity: lowercase ASCII letters, digits and hyphens",
        ));
    }

    if registration.audience.trim().is_empty() {
        return Err(invalid(
            "audience must not be empty; an unaudienced token is a token for anybody",
        ));
    }

    if registration.jwks_uri.trim().is_empty() {
        return Err(invalid("jwks_uri must not be empty"));
    }

    if registration.algorithms.is_empty() {
        return Err(invalid(
            "algorithms must name at least one; an empty list would let the token header choose",
        ));
    }

    if registration.store.trim().is_empty() {
        return Err(invalid("store must not be empty"));
    }

    if registration.max_key_age_seconds == 0 {
        return Err(invalid("max_key_age_seconds must not be zero"));
    }

    Ok(())
}
