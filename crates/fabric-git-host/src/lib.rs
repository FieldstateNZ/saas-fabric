//! Authenticating to a Git host as a GitHub App.
//!
//! ```text
//! GitCredential          what the platform was given
//!       ↓
//! BearerSource           mints, caches and expires an installation token
//!       ↓
//! Authorization: Bearer  what every call to the host carries
//! ```
//!
//! # Why this is its own crate
//!
//! SaaS Fabric connects to a Git host for two independent reasons — client
//! desired state, and platform desired state — and the specification requires
//! them to be **separate GitHub Apps, independently installable, configurable
//! and removable**. Two integrations, two Apps, two repositories, two
//! credentials.
//!
//! What is not two things is *how you turn a private key into a bearer*. That
//! is one exchange with one endpoint, with a cache whose correctness is subtle
//! (a stated expiry that must be read rather than assumed, a wall-clock
//! remaining lifetime measured against a monotonic deadline, an invalidation
//! path for a token that stops working inside its stated life). Two copies of
//! it would be two copies of the platform's credential-minting code, and a fix
//! to one would silently miss the other.
//!
//! So the *credential* is shared and the *integrations* are not. Nothing here
//! knows which repository it is authenticating to, what is stored there, or
//! what the caller intends to do with it.
//!
//! # What this crate does not decide
//!
//! Which repository, which paths, which API, or what a failure means to the
//! caller. It reports [`TokenError`], and each adapter maps that into its own
//! vocabulary — because "the credential was refused" means something different
//! to a client repository than it does to a platform one.

mod bearer;
mod credential;
mod errors;

pub use bearer::{sign_app_assertion, BearerSource};
pub use credential::GitCredential;
pub use errors::TokenError;
