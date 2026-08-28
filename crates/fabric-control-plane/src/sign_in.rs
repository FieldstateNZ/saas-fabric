//! Signing an operator in, without the console meeting the identity provider.
//!
//! # Why this is a port rather than a fetch from the browser
//!
//! The console's content security policy is `default-src 'self'`, and its
//! stated contract is that it talks to this API and to nothing else. A browser
//! may *navigate* to the identity provider — a top-level navigation is not
//! governed by `connect-src` — but it may not `fetch` it, so the console
//! cannot redeem an authorization code itself.
//!
//! That constraint turns out to be the better design anyway. The console never
//! learns the provider's address, the redemption happens on a server where the
//! redirect URI cannot be chosen by whoever made the request, and the console
//! image stays free of any deployment's identity configuration.
//!
//! # PKCE, and who holds which half
//!
//! The browser generates the verifier and keeps it. It sends only the
//! challenge to the provider, and sends the verifier here — to this platform's
//! own API — when redeeming. So the verifier never reaches the provider except
//! through the redemption it authorises, which is exactly what PKCE is for:
//! proof that the party redeeming the code is the party that requested it.

use std::sync::Arc;

use async_trait::async_trait;

/// An access token the identity provider issued to an operator.
///
/// Deliberately not a refresh token. The console holds this in memory for the
/// life of a page, and signs in again when it expires; a refresh token in a
/// browser is a long-lived credential in the least defensible place to keep
/// one.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IssuedToken {
    /// The token itself.
    pub access_token: String,

    /// How long it is good for, in seconds.
    pub expires_in: u64,
}

/// Why an authorization code could not be redeemed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignInError {
    /// The provider refused the code, the verifier, or the redirect.
    ///
    /// The provider's own message is deliberately not carried through: it is
    /// returned to a browser, and an upstream error body is not something to
    /// reflect unread.
    #[error("the identity provider refused the sign-in")]
    Refused,

    /// The provider could not be reached, or answered unintelligibly.
    #[error("the identity provider is unreachable")]
    Unavailable,
}

/// Redeems an authorization code for an operator's token.
///
/// One method and one accessor. There is no sign-*out* here, because there is
/// nothing to sign out of: this platform holds no session, so ending one is
/// the console dropping a token it kept in memory. Revoking the provider's own
/// session is the provider's screen, not this API's.
#[async_trait]
pub trait OperatorSignIn: Send + Sync {
    /// Where the browser is sent to authenticate.
    ///
    /// Opaque to this crate. It is the provider's authorization endpoint, and
    /// which URL that is belongs to whichever adapter speaks the provider's
    /// protocol.
    fn authorization_endpoint(&self) -> &str;

    /// Exchanges an authorization code for an access token.
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::Refused`] when the provider rejects the
    /// redemption, and [`SignInError::Unavailable`] when it cannot be reached.
    async fn redeem(&self, code: &str, verifier: &str) -> Result<IssuedToken, SignInError>;
}

/// Everything the console needs to start and finish a sign-in.
///
/// Assembled by the composition root, because the provider's address and the
/// console's own origin are both deployment facts and neither belongs to this
/// crate.
pub struct SignInSurface {
    /// Redeems the authorization code.
    pub provider: Arc<dyn OperatorSignIn>,

    /// The client the console authenticates as.
    pub client_id: String,

    /// Where the provider returns the browser.
    pub redirect_uri: String,
}
