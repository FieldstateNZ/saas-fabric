//! A hostname a client's application is reached on.

use std::fmt;

use fabric_core::IdentifierError;

/// A DNS hostname, such as `www.example.com`.
///
/// Every label is checked with the same rule as a
/// [`ClientId`](super::ClientId), so a host is a sequence of DNS labels and
/// nothing else: no scheme, no port, no path, no trailing dot, no wildcard.
/// The value reaches routing configuration eventually, and a host that
/// silently carried `https://` or `:8443` would produce a route that never
/// matches — a failure that shows up as an outage rather than as a validation
/// error.
///
/// Case is rejected rather than folded, for the reason
/// [`TenantId`](fabric_core::TenantId) gives: lowercasing here would make
/// `WWW.example.com` and `www.example.com` the same host in the document and
/// different strings in whatever consumed it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Host(String);

impl Host {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "host";

    /// The inclusive maximum length of a fully-qualified name, from the DNS
    /// specification.
    const MAX_LENGTH: usize = 253;

    /// Parses a hostname.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 253
    /// bytes, or if any dot-separated label breaks the DNS-label rule.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        let value = value.as_ref();

        if value.is_empty() {
            return Err(IdentifierError::Empty { kind: Self::KIND });
        }

        if value.len() > Self::MAX_LENGTH {
            return Err(IdentifierError::TooLong {
                kind: Self::KIND,
                max: Self::MAX_LENGTH,
                actual: value.len(),
            });
        }

        for label in value.split('.') {
            fabric_core::naming::parse_dns_label(Self::KIND, label)?;
        }

        Ok(Self(value.to_owned()))
    }

    /// Borrows the hostname as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Host {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Host {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<Host> for String {
    fn from(value: Host) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_fully_qualified_name() {
        assert!(Host::try_new("www.example.com").is_ok());
    }

    #[test]
    fn a_scheme_or_port_is_refused_rather_than_stripped() {
        assert!(Host::try_new("https://www.example.com").is_err());
        assert!(Host::try_new("www.example.com:8443").is_err());
    }

    #[test]
    fn an_empty_label_is_refused() {
        assert!(Host::try_new("www..example.com").is_err());
        assert!(Host::try_new("www.example.com.").is_err());
    }
}
