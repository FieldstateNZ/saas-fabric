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
//! established identity represent?** That answer comes from one place and one
//! place only — the `tenant_id` claim inside the bearer token (§10).
//!
//! Parsing a claim out of a token does not make this crate responsible for
//! authentication; the specification says so explicitly in §12.
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

mod bearer;
mod claims;
mod config;
mod errors;
mod extractor;
mod identity;
mod logging;
mod readers;
mod registration;
mod resolver;
#[cfg(test)]
mod resolver_tests;
mod token_reader;

pub use claims::TokenClaims;
pub use config::IdentityConfig;
pub use errors::IdentityError;
pub use identity::TenantIdentity;
pub use readers::{encode_unsigned_token, TrustedIngressReader, ValidatingReader, VerificationKeys};
pub use registration::build_identity;
pub use resolver::IdentityResolver;
pub use token_reader::TokenReader;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 1;
