//! Reading a token before anything about it is trusted.
//!
//! Two values are read from an unverified token: the `kid`, to select a key,
//! and the `iss`, to select a registration. Neither is *believed* — they
//! choose which trusted material the token is then checked against, and a
//! token that lies about either simply fails that check.
//!
//! This is the one place in the crate that reads an unchecked claim, and it is
//! its own module so that it stays the one place.

use base64::Engine as _;
use serde::Deserialize;

use crate::RefusalReason;

/// The issuer a token names, before any of it has been verified.
///
/// # Errors
///
/// Returns a [`RefusalReason`] when the token is not a JWT this crate can
/// read, or names no issuer.
pub(super) fn issuer_of(token: &str) -> Result<String, RefusalReason> {
    let payload = token.split('.').nth(1).ok_or(RefusalReason::Malformed)?;

    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| RefusalReason::Malformed)?;

    let claims: IssuerClaim = serde_json::from_slice(&decoded).map_err(|_| RefusalReason::Malformed)?;

    claims.iss.ok_or(RefusalReason::NoIssuer)
}

/// Only the claim this module is allowed to read.
///
/// Deliberately not the whole claim set: a type with a field for `tenant` or
/// `store_id` would be a type somebody could later be tempted to read one
/// from, and those come from the registry alone.
#[derive(Deserialize)]
struct IssuerClaim {
    /// The issuer, if the token names one.
    iss: Option<String>,
}
