//! One issuer string, in the only shape the registry accepts.

/// An issuer the registry names: exactly `https://<host>[:<port>]/realms/<realm>`.
///
/// The edge compares a token's `iss` to this string exactly (ADR 0019 G4),
/// so the registry refuses anything looser at construction rather than
/// leaving the platform to discover it. The shape is the platform contract's
/// (`saas-fabric-platform` `scripts/runtime_contract.py`, `ISSUER`): a
/// lowercase host of letters, digits, dots and hyphens, an optional port of
/// at most five digits, and a realm of letters, digits, `_` and `-`. No
/// path prefix, query, fragment or trailing slash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RegistryIssuer(String);

/// Why a value is not an issuer (or origin) the registry accepts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not https://<host>[:<port>]/realms/<realm>: {reason}")]
pub struct IssuerError {
    value: String,
    reason: &'static str,
}

impl IssuerError {
    pub(super) fn new(value: &str, reason: &'static str) -> Self {
        Self {
            value: value.to_owned(),
            reason,
        }
    }
}

impl RegistryIssuer {
    /// Parses an issuer string.
    ///
    /// # Errors
    ///
    /// [`IssuerError`] for any value outside the shape above.
    pub fn parse(value: &str) -> Result<Self, IssuerError> {
        let Some((origin, realm)) = value.split_once("/realms/") else {
            return Err(IssuerError::new(value, "no /realms/ segment"));
        };
        super::origin::check_origin(origin).map_err(|reason| IssuerError::new(value, reason))?;
        check_realm(realm).map_err(|reason| IssuerError::new(value, reason))?;
        Ok(Self(value.to_owned()))
    }

    /// The issuer string, exactly as a token's `iss` must state it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn check_realm(realm: &str) -> Result<(), &'static str> {
    if realm.is_empty() {
        return Err("the realm is empty");
    }
    if realm
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        Ok(())
    } else {
        Err("the realm may hold only letters, digits, '_' and '-'")
    }
}

impl TryFrom<String> for RegistryIssuer {
    type Error = IssuerError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RegistryIssuer> for String {
    fn from(issuer: RegistryIssuer) -> Self {
        issuer.0
    }
}

impl std::fmt::Display for RegistryIssuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
