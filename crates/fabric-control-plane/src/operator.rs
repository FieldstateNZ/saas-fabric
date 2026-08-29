//! Who is administering the platform.
//!
//! # This is not the runtime's identity model, and must not become it
//!
//! The runtime plane resolves a **tenant** from a bearer token and serves that
//! tenant's own data. Nothing it does requires authority over anything else.
//! An operator is the opposite: one person, with authority over every client,
//! whose identity has nothing to do with any tenant.
//!
//! So there is no `tenant_id` anywhere in this module, and no path by which one
//! could confer operator authority. A tenant administrator holding a perfectly
//! valid runtime token — including one with an administrator role in their own
//! realm — is not an operator here and cannot become one. ADR 0009 records
//! why that seam is worth a separate mechanism rather than a shared one.
//!
//! # The postures
//!
//! [`OidcOperators`] verifies a token the platform's own identity provider
//! issued and takes authority from a realm role. It is the production posture,
//! and the one that means the control plane does not depend on being
//! unreachable in order to be safe.
//!
//! [`TrustedHeaderOperators`] consumes an identity that the **operator network
//! boundary** has already established: the control plane is reachable only
//! from the operator plane (Tailscale today), and the proxy in front of it
//! authenticates the human and states who they are in a header.
//!
//! That is the same posture ADR 0002 chose for the runtime, and it carries the
//! same obligation: the header is trustworthy *because of where the service
//! sits*, and exposing this API anywhere else makes it trivially forgeable.
//! The configuration says `mode = "trusted_header"` explicitly for that reason
//! — a deployment has to state its posture rather than inherit one.

mod authenticator;
mod errors;
mod extractor;
mod identity;
mod oidc;
mod trusted_header;
#[cfg(test)]
mod trusted_header_tests;

pub use authenticator::OperatorAuthenticator;
pub use errors::OperatorAuthError;
pub use identity::Operator;
pub use oidc::{KeyHolder, OidcOperators, VerificationKeys};
pub use trusted_header::TrustedHeaderOperators;
