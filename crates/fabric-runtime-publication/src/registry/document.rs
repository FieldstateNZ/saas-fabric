//! `issuer-registry.json` as a whole.

use std::collections::BTreeSet;

use crate::canonical::to_canonical_bytes;

use super::{IssuerRegistration, RegistryGeneration, RegistryRefusal};

/// The registry shape this publisher writes. A breaking change ships as a
/// new number, never as a reinterpretation of this one.
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// One generation of the issuer registry.
///
/// The keys are the platform contract's (`schemaVersion`, `generation`,
/// `issuers`), which is why `schemaVersion` is camel case where every other
/// published document is snake case: the platform reads this file and its
/// registry check was written first. Registrations are sorted by issuer, so
/// one set of registrations has exactly one rendering, and two publishers
/// offering the same set produce the same bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerRegistryDocument {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    generation: RegistryGeneration,
    issuers: Vec<IssuerRegistration>,
}

impl IssuerRegistryDocument {
    /// The registry at `generation`, its registrations in canonical order.
    ///
    /// # Errors
    ///
    /// [`RegistryRefusal::DuplicateIssuer`] if one issuer is registered
    /// twice, even to the same tenant.
    pub fn new(
        generation: RegistryGeneration,
        mut issuers: Vec<IssuerRegistration>,
    ) -> Result<Self, RegistryRefusal> {
        issuers.sort();
        let mut seen = BTreeSet::new();
        if let Some(duplicate) = issuers
            .iter()
            .find(|registration| !seen.insert(&registration.issuer))
        {
            return Err(RegistryRefusal::DuplicateIssuer {
                issuer: duplicate.issuer.clone(),
            });
        }
        Ok(Self {
            schema_version: REGISTRY_SCHEMA_VERSION,
            generation,
            issuers,
        })
    }

    /// The registry shape these bytes were written in.
    pub(crate) const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Which generation this is.
    #[must_use]
    pub const fn generation(&self) -> RegistryGeneration {
        self.generation
    }

    /// The registrations, sorted by issuer.
    #[must_use]
    pub fn issuers(&self) -> &[IssuerRegistration] {
        &self.issuers
    }

    /// Canonical JSON, the formatting rule every published file shares.
    ///
    /// # Errors
    ///
    /// Only if this type's own `Serialize` fails, which it never does.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_canonical_bytes(self)
    }
}
