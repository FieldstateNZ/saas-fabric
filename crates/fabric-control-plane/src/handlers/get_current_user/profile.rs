//! Reading the presentation claims out of an accepted operator's bearer.
//!
//! # Not a verifier, and not to be reused as one
//!
//! This takes an [`Operator`], which only the extractor can produce, so the
//! deployment's authenticator has already verified this exact token. It
//! decodes the payload and nothing else: no signature, no issuer, no expiry,
//! no provider call. That is safe here only because of that order, which is
//! why it is private to `GET /api/user/current` and must stay so — nothing it
//! returns may decide authentication, a grant, a tenant or an audit name.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{Map, Value};

use crate::{ControlPlaneError, Operator};

/// What the shell is shown about the operator.
pub(super) struct Profile {
    /// `sub`, exactly as the token states it.
    pub(super) subject: String,

    /// `preferred_username`, when nonblank.
    pub(super) user_name: Option<String>,

    /// `name`, else the username, else `sub`.
    pub(super) display_name: String,

    /// `email`, when nonblank.
    pub(super) email: Option<String>,
}

/// The profile in `operator`'s verified bearer.
///
/// # Errors
///
/// [`ControlPlaneError::ProfileUnreadable`] when the payload is not a JSON
/// object or states no nonblank `sub`. Never a made-up profile: a token the
/// authenticator accepted and this cannot read is a fault to surface.
pub(super) fn read(operator: &Operator) -> Result<Profile, ControlPlaneError> {
    let claims = payload(operator.token().expose()).ok_or(ControlPlaneError::ProfileUnreadable)?;

    let subject = text(&claims, "sub").ok_or(ControlPlaneError::ProfileUnreadable)?;
    let user_name = text(&claims, "preferred_username");
    let display_name = text(&claims, "name")
        .or_else(|| user_name.clone())
        .unwrap_or_else(|| subject.clone());

    Ok(Profile {
        subject,
        email: text(&claims, "email"),
        user_name,
        display_name,
    })
}

/// The token's payload as a JSON object, if it has one.
fn payload(token: &str) -> Option<Map<String, Value>> {
    let mut segments = token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (segments.next(), segments.next(), segments.next(), segments.next())
    else {
        return None;
    };

    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    match serde_json::from_slice(&bytes).ok()? {
        Value::Object(claims) => Some(claims),
        _ => None,
    }
}

/// A claim's value, when it is a string that is not blank.
///
/// A claim of another shape reads as absent rather than failing the profile:
/// the optional fields are presentation, and only `sub` is required.
fn text(claims: &Map<String, Value>, name: &str) -> Option<String> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}
