//! Signing the assertion the installation-token endpoint authenticates.
//!
//! Shared with provisioning, which mints a token to *prove* an installation
//! works before recording it. One signer, so the two cannot drift.

use fabric_control_plane::RepositoryError;
use jsonwebtoken::{Algorithm, EncodingKey, Header};

/// How long the App JWT presented to the token endpoint is valid.
///
/// GitHub rejects anything over ten minutes. Nine leaves room for the clock
/// skew the `iat` backdating below also allows for.
const JWT_LIFETIME_SECONDS: u64 = 9 * 60;

/// How far `iat` is backdated.
///
/// GitHub rejects a JWT whose `iat` is in the future by its clock. A minute
/// covers ordinary skew between a cluster node and GitHub, and is what their
/// own documentation suggests.
const JWT_BACKDATE_SECONDS: u64 = 60;

/// Builds the App JWT the token endpoint authenticates.
///
/// # Errors
///
/// Returns [`RepositoryError::NotPermitted`] if the key is not a private key
/// this can sign with. The underlying error is deliberately dropped:
/// `jsonwebtoken` includes the offending input in some of its messages, and
/// the offending input here is the private key.
pub(crate) fn build(app_id: &str, private_key: &str, now_unix: u64) -> Result<String, RepositoryError> {
    let issued_at = now_unix.saturating_sub(JWT_BACKDATE_SECONDS);

    let claims = serde_json::json!({
        "iat": issued_at,
        "exp": issued_at + JWT_BACKDATE_SECONDS + JWT_LIFETIME_SECONDS,
        "iss": app_id,
    });

    let key = EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|_| RepositoryError::NotPermitted)?;

    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|_| RepositoryError::NotPermitted)
}
