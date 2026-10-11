//! Who is asking, and the credential they asked with.

use std::fmt;
use std::future::{self, Future};
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use fabric_identity::{extract_bearer, IdentityError, IdentityResolver, TenantIdentity};
use http::request::Parts;

/// The caller's own bearer token, carried to the authorization front.
///
/// ADR 0016: the user's JWT remains the credential at the Fabric boundary and
/// no workload identity exists anywhere in the authorization path. Layer 2
/// therefore forwards *this* token and nothing of the runtime's own.
///
/// `Debug` never prints it: an identity in a log line is fine, a replayable
/// credential is not.
#[derive(Clone)]
pub struct CallerCredential(String);

impl CallerCredential {
    /// Wraps a bearer token as received.
    #[must_use]
    pub const fn new(token: String) -> Self {
        Self(token)
    }

    /// The token, for the one adapter that sends it onward.
    #[must_use]
    pub fn bearer(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CallerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CallerCredential(<redacted>)")
    }
}

/// An authenticated caller: the identity the tenant came from, and the token.
///
/// One extractor for both, so a handler cannot hold an identity that came from
/// one request and a credential that came from another.
#[derive(Debug, Clone)]
pub struct Caller {
    /// The tenant identity the resolver established.
    identity: TenantIdentity,

    /// The bearer token that identity was read from.
    credential: CallerCredential,
}

impl Caller {
    /// The tenant identity.
    #[must_use]
    pub const fn identity(&self) -> &TenantIdentity {
        &self.identity
    }

    /// The caller's own credential.
    #[must_use]
    pub const fn credential(&self) -> &CallerCredential {
        &self.credential
    }
}

/// Resolves the identity first, so every refusal it can produce is exactly the
/// one the `TenantIdentity` extractor produced before this type existed. Pure,
/// for the same reason that extractor is: nothing here may await.
impl<S> FromRequestParts<S> for Caller
where
    S: Send + Sync,
    Arc<IdentityResolver>: FromRef<S>,
{
    type Rejection = IdentityError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        let resolver = Arc::<IdentityResolver>::from_ref(state);

        future::ready(resolver.resolve(&parts.headers).and_then(|identity| {
            let token = extract_bearer(&parts.headers)?;

            Ok(Self {
                identity,
                credential: CallerCredential::new(token.to_owned()),
            })
        }))
    }
}

#[cfg(test)]
pub(crate) mod fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credential_never_reaches_a_debug_line() {
        let credential = CallerCredential::new("secret.token.value".to_owned());

        assert!(!format!("{credential:?}").contains("secret"));
    }
}
