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
//! # One posture, and only one
//!
//! [`OidcOperators`] verifies a token the platform's own identity provider
//! issued and takes authority from a realm role. There is no alternative, and
//! deliberately no development shortcut beside it.
//!
//! A trusted-header posture used to sit here, consuming an identity the
//! operator-plane proxy asserted. It was removed rather than kept for local
//! use, for two reasons that turn out to be the same reason. It was safe only
//! because of *where the service sat* — nothing in the application enforced
//! that, so the same container on another network authenticated anybody. And
//! it asserted a name while lending nothing: the platform now acts on the
//! identity provider with an operator's own bearer (ADR 0012), and a posture
//! that cannot supply one leaves half the control plane unable to work.
//!
//! A development posture that cannot do what production does is a development
//! posture that hides exactly the failures worth finding early.

mod authenticator;
mod errors;
mod extractor;
mod identity;
mod oidc;

pub use authenticator::OperatorAuthenticator;
pub use errors::OperatorAuthError;
pub use identity::{Operator, OperatorToken};
pub use oidc::{KeyHolder, OidcOperators, VerificationKeys};
