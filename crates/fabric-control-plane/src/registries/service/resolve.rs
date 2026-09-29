//! A registration, held to the rules before anything is asked of a registry.

use crate::git_integration::SecretValue;
use crate::registries::endpoint::distribution;
use crate::registries::{RegistryCredential, RegistryFailure, RegistryHost, RegistryKind};

/// What an operator registers.
pub(crate) struct Registration {
    /// The kind, which fixes a hosted registry's host and endpoint.
    pub(crate) kind: RegistryKind,

    /// A `distribution` registry's endpoint; none for a hosted kind.
    pub(crate) endpoint: Option<String>,

    /// The registry account, given with a token or not at all.
    pub(crate) username: Option<String>,

    /// The token. Write-only: nothing returns it.
    pub(crate) token: Option<SecretValue>,
}

/// A registration the rules accepted, before it is proven.
pub(super) struct Resolved {
    /// What it will be known by.
    pub(super) host: RegistryHost,

    /// Its kind.
    pub(super) kind: RegistryKind,

    /// Where it is served.
    pub(super) endpoint: String,

    /// Its credential, if one was given.
    pub(super) credential: Option<RegistryCredential>,
}

/// Resolves a registration's host and endpoint by its kind, and checks its
/// credential is given whole.
pub(super) fn resolve(registration: Registration) -> Result<Resolved, RegistryFailure> {
    let invalid = |message: &str| RegistryFailure::Invalid(message.to_owned());
    let (host, endpoint) = match (registration.kind.hosted(), registration.endpoint) {
        (Some(_), Some(_)) => {
            return Err(RegistryFailure::Invalid(format!(
                "a {} registry's host and endpoint are fixed by its kind; send no endpoint",
                registration.kind.as_str()
            )))
        }
        (Some(hosted), None) => (
            RegistryHost::parse(hosted.host).map_err(RegistryFailure::Invalid)?,
            hosted.endpoint.to_owned(),
        ),
        (None, Some(endpoint)) => distribution(&endpoint).map_err(RegistryFailure::Invalid)?,
        (None, None) => return Err(invalid("a distribution registry needs its HTTPS endpoint")),
    };

    let credential = match (registration.username, registration.token) {
        (None, None) => None,
        (Some(username), Some(token)) => Some(credential(username, token)?),
        _ => return Err(invalid("a credential is a username and a token, given together")),
    };

    Ok(Resolved {
        host,
        kind: registration.kind,
        endpoint,
        credential,
    })
}

/// A credential, held to the rules: a username `Basic` can carry, and a
/// token that is not blank. The message never repeats either.
pub(super) fn credential(
    username: String,
    token: SecretValue,
) -> Result<RegistryCredential, RegistryFailure> {
    if username.trim().is_empty() || username.contains(':') || username.chars().any(char::is_control) {
        return Err(RegistryFailure::Invalid(
            "a credential's username is not blank and holds no ':' or control character".to_owned(),
        ));
    }
    if token.expose().trim().is_empty() {
        return Err(RegistryFailure::Invalid(
            "a credential's token is not blank".to_owned(),
        ));
    }
    Ok(RegistryCredential::new(username, token))
}
