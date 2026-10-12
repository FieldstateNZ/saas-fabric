//! Which scheme a client on `.internal` hosts is reached over (D01-15).

use std::fmt;

/// The scheme a client's `.internal` callbacks use, declared per client.
///
/// # Why it is declared rather than inferred
///
/// An `.internal` name says the host is private, not whether the network it
/// is reached on terminates TLS: no public certificate authority issues for
/// it, but a private one can, and `privateNetwork` admits both. Inferring
/// `http` from one deployment's port-80 gateway would be wrong for the next
/// one, and Keycloak matches a callback's scheme exactly, so a wrong guess is
/// a client nobody can sign in to. A closed set of two, so nothing else can
/// be spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivateNetworkScheme {
    /// Plain HTTP, for a private network with no TLS listener.
    Http,

    /// HTTPS, for a private network that terminates TLS, typically with a
    /// private certificate authority.
    Https,
}

impl PrivateNetworkScheme {
    /// Both schemes, in a stable order.
    pub const ALL: [Self; 2] = [Self::Http, Self::Https];

    /// The scheme as a URI spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

impl fmt::Display for PrivateNetworkScheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::PrivateNetworkScheme;

    #[test]
    fn only_http_and_https_are_spelled() {
        let parse = |text: &str| serde_norway::from_str::<PrivateNetworkScheme>(text);

        assert_eq!(parse("http").unwrap(), PrivateNetworkScheme::Http);
        assert_eq!(parse("https").unwrap(), PrivateNetworkScheme::Https);
        for refused in ["HTTP", "ftp", "", "http://", "tls"] {
            assert!(parse(refused).is_err(), "{refused}");
        }
    }
}
