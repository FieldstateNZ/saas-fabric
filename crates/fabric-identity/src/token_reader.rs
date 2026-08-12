//! The seam between "we have a token string" and "we have claims".

use crate::{IdentityError, TokenClaims};

/// Turns a bearer token into its claims.
///
/// This is a trait rather than a function because the two implementations
/// represent genuinely different security postures, and which one a deployment
/// uses is a decision that should be visible in the composition root rather
/// than buried in a config flag deep in a call stack.
///
/// It is deliberately synchronous. Both implementations work from keys already
/// held in memory, so there is no I/O on the request path — which matters,
/// because this runs on every single request. A future implementation that
/// needed to fetch a rotating key set should fetch it on a background task and
/// swap it in, exactly as the tenant registry does with bindings, rather than
/// making this method async and putting a network call in front of every
/// request.
pub trait TokenReader: Send + Sync {
    /// Reads the claims from a bearer token.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::MalformedToken`] when the token is not a
    /// well-formed JWT, [`IdentityError::ExpiredToken`] when it has expired, or
    /// [`IdentityError::UnverifiedToken`] when a signature or registered-claim
    /// check fails.
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError>;

    /// A short name for this reader, used in startup logging so the deployed
    /// security posture is visible in the logs.
    fn describe(&self) -> &'static str;
}
