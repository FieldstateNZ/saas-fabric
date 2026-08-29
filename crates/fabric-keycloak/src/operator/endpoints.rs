//! Which of a realm's two addresses each endpoint is built from.
//!
//! The endpoints a realm publishes, derived from two URLs rather than one.
//!
//! # Why two
//!
//! A realm's issuer is what appears in the tokens it mints, and what a
//! *browser* is sent to. Where **this process** reaches the same realm can be
//! a different address entirely, and on a cluster it usually is: the issuer is
//! a public hostname resolved outside, and the pod has a service address that
//! nothing outside can resolve.
//!
//! Collapsing them is the mistake this type exists to prevent. Use the issuer
//! for everything and the pod cannot fetch the signing keys, so every operator
//! is refused and the log says only that no key set arrived. Use the internal
//! address for everything and the `iss` in a real token never matches, so
//! every operator is refused and the log says the token is not from this
//! realm. Both are hours of looking in the wrong place.
//!
//! They default to the same value, because most deployments genuinely have one
//! URL. Each is still *derived* into its endpoints rather than stated, so a
//! deployment cannot supply four URLs that disagree.

/// The three endpoints a sign-in uses.
pub(super) struct Endpoints {
    /// Where the browser authenticates.
    pub(super) authorization: String,

    /// Where this process redeems an authorization code.
    pub(super) token: String,

    /// Where this process reads the realm's signing keys.
    pub(super) jwks: String,
}

/// Derives all three from the two addresses a deployment states.
pub(super) fn derive(issuer: &str, reachable_at: &str) -> Endpoints {
    let issuer = issuer.trim().trim_end_matches('/');
    let reachable_at = reachable_at.trim().trim_end_matches('/');

    Endpoints {
        // The browser goes here, so it is the issuer's host.
        authorization: format!("{issuer}/protocol/openid-connect/auth"),

        // This process goes here, so it is the reachable host.
        token: format!("{reachable_at}/protocol/openid-connect/token"),
        jwks: format!("{reachable_at}/protocol/openid-connect/certs"),
    }
}
