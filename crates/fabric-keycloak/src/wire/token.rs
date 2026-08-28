//! Keycloak's token endpoint response.

/// What the token endpoint returns for a client-credentials grant.
///
/// The one wire type carrying a secret. It is never logged, never stored
/// beyond the token cache, and never returned upward — the port this crate
/// implements has no operation that could hand a token to a caller.
#[derive(serde::Deserialize)]
pub(crate) struct TokenResponse {
    /// The bearer token to present to the admin API.
    pub(crate) access_token: String,

    /// How many seconds it remains valid.
    ///
    /// Keycloak's default for a service account is short — a minute or two —
    /// so this is read rather than assumed. A cache that guessed would either
    /// re-authenticate on every call or present an expired token and fail a
    /// whole sweep on a 401.
    pub(crate) expires_in: u64,
}

// No `Debug`: deriving one would put an access token into any error or log
// line that formatted a value containing this type. There is nothing about it
// worth printing.
