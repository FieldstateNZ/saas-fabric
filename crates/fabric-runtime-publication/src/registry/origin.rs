//! Where every client realm's issuer lives: the identity provider's public
//! origin.

use super::issuer::{IssuerError, RegistryIssuer};

/// The public origin client realms are issued under: `https://<host>[:<port>]`.
///
/// A client's desired state names its realm, not its issuer -- the issuer is
/// the identity provider's public URL plus `/realms/<realm>`, and only the
/// deployment knows that URL. Holding it as a checked type means every
/// issuer the registry generates is valid by construction, and a deployment
/// that states a malformed origin is refused at startup, not at the edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerOrigin(String);

impl IssuerOrigin {
    /// Parses an origin.
    ///
    /// # Errors
    ///
    /// [`IssuerError`] for anything but `https://<host>[:<port>]` -- a path,
    /// a trailing slash, an uppercase host or an `http` scheme included.
    pub fn parse(value: &str) -> Result<Self, IssuerError> {
        check_origin(value).map_err(|reason| IssuerError::new(value, reason))?;
        Ok(Self(value.to_owned()))
    }

    /// The issuer of `realm` under this origin.
    ///
    /// # Errors
    ///
    /// [`IssuerError`] if `realm` holds a character the registry refuses.
    pub fn issuer(&self, realm: &str) -> Result<RegistryIssuer, IssuerError> {
        RegistryIssuer::parse(&format!("{}/realms/{realm}", self.0))
    }

    /// The origin string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn check_origin(origin: &str) -> Result<(), &'static str> {
    let Some(authority) = origin.strip_prefix("https://") else {
        return Err("the scheme is not https");
    };
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty() {
        return Err("the host is empty");
    }
    if !host
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-')
    {
        return Err("the host may hold only lowercase letters, digits, '.' and '-', and no path");
    }
    match port {
        Some(port)
            if port.is_empty() || port.len() > 5 || !port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Err("the port is not one to five digits")
        }
        _ => Ok(()),
    }
}
