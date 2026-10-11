//! The issuer registry: which identity issuer names which tenant, as one
//! generation Fabric produces and platform composition applies (D01-4).
//!
//! Carried with the tenants document rather than as a fourth document (ADR
//! 0018, "The issuer registry"): it is written beside `tenants.json` -- in
//! Kubernetes, a fourth data key of the tenants `ConfigMap` -- and the
//! publication record names its generation and digest, so the registry and
//! the tenant list it was generated from are one generation. Fabric never
//! writes the edge: the platform reads this registry and renders both the
//! gateway's issuer allow-list and the runtime's `[identity].trusted_issuers`
//! from it (ADR 0019 G4a).

mod document;
mod generation;
mod issuer;
mod origin;
mod plan;
mod recorded;
mod refusal;
mod registration;

#[cfg(test)]
mod registry_tests;

pub use document::{IssuerRegistryDocument, REGISTRY_SCHEMA_VERSION};
pub use generation::RegistryGeneration;
pub use issuer::{IssuerError, RegistryIssuer};
pub use origin::IssuerOrigin;
pub(crate) use plan::decide;
pub use plan::RegistryPlan;
pub use recorded::RecordedRegistry;
pub use refusal::RegistryRefusal;
pub use registration::IssuerRegistration;

/// The registry's file name, beside `tenants.json` -- and, in the Kubernetes
/// adapter, a data key of the tenants `ConfigMap`, so it is projected in the
/// same volume swap as the tenants payload and the record naming both.
pub const ISSUER_REGISTRY_FILE: &str = "issuer-registry.json";
