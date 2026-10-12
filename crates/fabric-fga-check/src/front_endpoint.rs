//! Where the authorization front's runtime surface is, and how it is reached.

use reqwest::Url;

/// How the hop that carries the caller's bearer token is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transport {
    /// HTTPS with the peer certificate and hostname verified against the
    /// host's trust roots (ADR 0005), and no plaintext fallback. The only
    /// transport a live endpoint can have (ADR 0027 §2b).
    VerifiedTls,

    /// Plain HTTP to a loopback address. Compiled into test builds only, so no
    /// production code path can construct it.
    #[cfg(test)]
    LoopbackPlaintext,
}

/// The base address of the front's runtime surface, for example
/// `https://fabric-openfga.authz.svc:8443`.
///
/// # Only verified TLS outside tests (ADR 0027 §2b)
///
/// The request carries the caller's bearer token, so the hop must be TLS with
/// the peer verified. [`Self::parse`] accepts `https` only; an `http` address
/// is refused, whether or not its host is loopback. NetworkPolicy does not
/// replace transport protection.
///
/// Plaintext exists only behind `loopback_plaintext`, which is compiled into
/// this crate's own test builds and refuses any non-loopback address.
///
/// An address carrying credentials, a query or a fragment is a template that
/// went wrong, and is refused rather than repaired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontEndpoint {
    /// The base address, without a trailing `/`.
    base: String,

    /// How the hop is protected.
    transport: Transport,
}

impl FrontEndpoint {
    /// Parses a live base address.
    ///
    /// # Errors
    ///
    /// Returns a message if the value is not an `https` URL, or carries a user,
    /// password, query or fragment.
    pub fn parse(value: &str) -> Result<Self, String> {
        let url = Url::parse(value).map_err(|error| format!("front endpoint: {error}"))?;

        if url.scheme() != "https" {
            return Err(format!(
                "front endpoint must be https: the request carries the caller's token, \
                 so {} is refused",
                url.scheme()
            ));
        }

        Self::from_url(&url, Transport::VerifiedTls)
    }

    /// A plaintext endpoint on loopback, for this crate's tests only.
    ///
    /// # Errors
    ///
    /// Returns a message if `address` is not a loopback address.
    #[cfg(test)]
    pub(crate) fn loopback_plaintext(address: std::net::SocketAddr) -> Result<Self, String> {
        if !address.ip().is_loopback() {
            return Err(format!("{address} is not loopback; plaintext is refused"));
        }

        let url = Url::parse(&format!("http://{address}")).map_err(|error| error.to_string())?;

        Self::from_url(&url, Transport::LoopbackPlaintext)
    }

    /// The checks every address shares.
    fn from_url(url: &Url, transport: Transport) -> Result<Self, String> {
        if !url.username().is_empty() || url.password().is_some() {
            return Err("front endpoint must not carry credentials".to_owned());
        }

        if url.query().is_some() || url.fragment().is_some() {
            return Err("front endpoint must not carry a query or fragment".to_owned());
        }

        Ok(Self {
            base: url.as_str().trim_end_matches('/').to_owned(),
            transport,
        })
    }

    /// The address of `POST /v1/check`.
    #[must_use]
    pub fn check_url(&self) -> String {
        format!("{}/v1/check", self.base)
    }

    /// Whether the client must refuse anything but HTTPS.
    pub(crate) const fn requires_tls(&self) -> bool {
        matches!(self.transport, Transport::VerifiedTls)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;

    #[test]
    fn an_https_address_is_accepted_and_requires_tls() {
        let endpoint = FrontEndpoint::parse("https://front.test.invalid:8443/").unwrap();

        assert_eq!(endpoint.check_url(), "https://front.test.invalid:8443/v1/check");
        assert!(endpoint.requires_tls());
    }

    #[test]
    fn http_to_a_non_loopback_host_is_refused() {
        for value in ["http://front.test.invalid:8080", "http://10.0.0.7:8080"] {
            assert!(
                FrontEndpoint::parse(value).is_err_and(|message| message.contains("must be https")),
                "{value}"
            );
        }
    }

    #[test]
    fn http_to_loopback_is_refused_outside_the_test_path() {
        for value in [
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "http://[::1]:8080",
        ] {
            assert!(FrontEndpoint::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn the_test_path_accepts_loopback_plaintext_only() {
        let loopback: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let endpoint = FrontEndpoint::loopback_plaintext(loopback).unwrap();

        assert!(!endpoint.requires_tls());
        assert!(FrontEndpoint::loopback_plaintext("10.0.0.7:8080".parse().unwrap()).is_err());
    }

    #[test]
    fn an_address_that_is_not_a_url_is_refused() {
        assert!(FrontEndpoint::parse("ftp://front.test.invalid").is_err());
        assert!(FrontEndpoint::parse("front.test.invalid:8443").is_err());
    }

    #[test]
    fn an_address_carrying_credentials_a_query_or_a_fragment_is_refused() {
        for value in [
            "https://user:pass@front.test.invalid",
            "https://front.test.invalid/?store=x",
            "https://front.test.invalid/#x",
        ] {
            assert!(FrontEndpoint::parse(value).is_err(), "{value}");
        }
    }
}
