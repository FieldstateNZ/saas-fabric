//! The seam behind operator authentication.

use http::HeaderMap;

use crate::operator::{Operator, OperatorAuthError};

/// Establishes which operator a request belongs to.
///
/// # Why a trait for one implementation
///
/// Because the implementation is expected to change and the rest of the crate
/// must not. Today the platform's operator boundary is a network one, so
/// [`TrustedHeaderOperators`](super::TrustedHeaderOperators) consumes what the
/// proxy established. When the platform can issue operator credentials of its
/// own — through Keycloak, or something else — that is a second implementation
/// of this trait and no handler changes.
///
/// The seam also states the rule structurally: this takes a
/// [`HeaderMap`] and returns an [`Operator`], so there is
/// no signature by which a tenant, a claim, or a runtime identity could be
/// consulted.
pub trait OperatorAuthenticator: Send + Sync {
    /// Establishes the operator, or refuses the request.
    ///
    /// # Errors
    ///
    /// Returns [`OperatorAuthError`] if no identity was presented, or if the
    /// one presented is not a platform operator.
    fn authenticate(&self, headers: &HeaderMap) -> Result<Operator, OperatorAuthError>;

    /// A short description of the posture, for the startup log.
    ///
    /// Must not contain the allowlist or anything else that would put operator
    /// identities in a log line nobody asked for.
    fn describe(&self) -> String;
}
