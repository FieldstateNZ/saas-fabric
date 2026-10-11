//! One line of the registry.

use fabric_core::TenantId;

use super::RegistryIssuer;

/// One issuer, and the tenant every token it issues belongs to.
///
/// Field for field the shape of `fabric_identity::TrustedIssuer`, so the
/// runtime's `[identity].trusted_issuers` can be rendered from the registry
/// without translation -- which is what makes the edge's allow-list and the
/// runtime's tenant binding one fact rather than two (ADR 0019 G4a).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerRegistration {
    /// The value a token's `iss` must equal.
    pub issuer: RegistryIssuer,
    /// The tenant that issuer's tokens belong to.
    pub tenant: TenantId,
}
