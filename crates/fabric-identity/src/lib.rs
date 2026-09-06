//! Derives the canonical tenant identity context from a bearer token.
//!
//! # What this crate is, and is not
//!
//! This crate **does not authenticate anybody**. Authentication happens at the
//! platform edge, in the gateway, against whichever identity provider the
//! deployment uses. By the time a request reaches the runtime plane it has
//! already been authenticated (specification §8, §9).
//!
//! What this crate does is answer the *next* question: **which tenant does this
//! established identity represent?**
//!
//! Parsing a claim out of a token does not make this crate responsible for
//! authentication; the specification says so explicitly in §12.
//!
//! # The issuer names the tenant; the claim only agrees
//!
//! The answer comes from [`IdentityConfig::trusted_issuers`]: the token's `iss`
//! is looked up in a registry this deployment configured, and **the tenant is
//! the registration's**. An unregistered issuer is refused, and a token with no
//! `iss` is refused rather than treated as unregistered-but-harmless.
//!
//! The `tenant_id` claim is still **required**, and it must equal the tenant the
//! registration names or the token is refused. It is a consistency check and
//! never the source. This crate verifies nothing itself, so a disagreement is
//! the only signal it will ever get that the edge and this registry have
//! diverged — and a token whose issuer says `acme` while its claim says `globex`
//! is not a request to disambiguate, it is a request to pick, and picking is the
//! bug. See ADR 0019 §2, and [`TrustedIssuer`] for why the tenant is
//! registry-derived rather than claimed.
//!
//! An earlier version of this paragraph said the tenant came "from one place and
//! one place only — the `tenant_id` claim". That was true, and it was the
//! cross-tenant hole ADR 0019 closes.
//!
//! # No tenant header, ever
//!
//! There is deliberately no code path here that reads a tenant from a request
//! header. §11 requires a single authoritative tenant context, and two sources
//! of truth is exactly the ambiguity that requirement exists to prevent. By
//! default a request that carries `X-Tenant-Id` is *rejected* rather than
//! quietly ignored, so a caller who believes the header works finds out
//! immediately. See [`IdentityConfig::reject_tenant_header`].
//!
//! # Identity-provider independence
//!
//! Nothing here knows what Keycloak is (§24). The contract is a trusted bearer
//! token plus a canonical tenant claim, so the identity implementation can be
//! swapped for Entra ID, Auth0, or a customer's own OIDC broker without
//! touching the runtime plane.
//!
//! # Choosing a token reader
//!
//! [`TrustedIngressReader`] is the canonical implementation and the default. It
//! follows the architectural contract directly: the edge authenticates, the
//! runtime consumes the result. It parses claims and checks expiry, and does
//! not re-validate what the gateway has already validated.
//!
//! [`ValidatingReader`] adds signature verification for deployments that want
//! **defence in depth** — a second layer over sound network policy, not a
//! replacement for it. If an untrusted client can reach the runtime directly,
//! that is a network policy failure and belongs to be fixed there; verifying
//! signatures here would mask it while leaving every other unauthenticated path
//! into the plane open.
//!
//! Neither reader performs issuer discovery, JWKS fetching, or anything else
//! that would make this crate a partial identity provider. Even in
//! defence-in-depth mode, keys arrive as a snapshot built outside the request
//! path.
//!
//! The tenant binding above is a property of the **resolver**, not of either
//! reader, so both postures reach it by the same path and neither can be
//! deployed without it. Matching the issuer against a registry is not
//! verification: it is a comparison against a string the deployment configured,
//! and it fetches nothing (§24).

mod bearer;
mod claims;
mod config;
mod errors;
mod extractor;
mod identity;
pub mod logging;
mod readers;
mod registration;
mod resolver;
#[cfg(test)]
mod resolver_tests;
mod token_reader;

pub use claims::TokenClaims;
pub use config::IdentityConfig;
pub use errors::IdentityError;
pub use identity::{TenantIdentity, TrustedIssuer};
pub use readers::{
    encode_unsigned_token, LeewaySeconds, TrustedIngressReader, ValidatingReader, VerificationKeys,
};
pub use registration::build_identity;
pub use resolver::IdentityResolver;
pub use token_reader::TokenReader;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 1;
