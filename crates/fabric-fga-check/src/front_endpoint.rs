//! Where the authorization front's runtime surface is.

use reqwest::Url;

/// The base address of the front's runtime surface, for example
/// `http://fabric-openfga.authz.svc:8080`.
///
/// Parsed rather than held as a string, because a bearer token is sent to it:
/// an address carrying credentials, a query or a fragment is a template that
/// went wrong, and is refused rather than repaired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontEndpoint(String);

impl FrontEndpoint {
    /// Parses a base address.
    ///
    /// # Errors
    ///
    /// Returns a message if the value is not an `http` or `https` URL, or
    /// carries a user, password, query or fragment.
    pub fn parse(value: &str) -> Result<Self, String> {
        let url = Url::parse(value).map_err(|error| format!("front endpoint: {error}"))?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!(
                "front endpoint must be http or https, not {}",
                url.scheme()
            ));
        }

        if !url.username().is_empty() || url.password().is_some() {
            return Err("front endpoint must not carry credentials".to_owned());
        }

        if url.query().is_some() || url.fragment().is_some() {
            return Err("front endpoint must not carry a query or fragment".to_owned());
        }

        Ok(Self(url.as_str().trim_end_matches('/').to_owned()))
    }

    /// The address of `POST /v1/check`.
    #[must_use]
    pub fn check_url(&self) -> String {
        format!("{}/v1/check", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_slash_does_not_double_up() {
        let endpoint = FrontEndpoint::parse("http://front.test.invalid:8080/").unwrap();

        assert_eq!(endpoint.check_url(), "http://front.test.invalid:8080/v1/check");
    }

    #[test]
    fn an_address_that_is_not_http_is_refused() {
        assert!(FrontEndpoint::parse("ftp://front.test.invalid").is_err());
        assert!(FrontEndpoint::parse("front.test.invalid:8080").is_err());
    }

    #[test]
    fn an_address_carrying_credentials_a_query_or_a_fragment_is_refused() {
        for value in [
            "http://user:pass@front.test.invalid",
            "http://front.test.invalid/?store=x",
            "http://front.test.invalid/#x",
        ] {
            assert!(FrontEndpoint::parse(value).is_err(), "{value}");
        }
    }
}
