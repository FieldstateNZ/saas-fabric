//! Validating the address a chart index read is about to start from.

use fabric_platform_management::RegistryError;

use super::{decide, shown, Transport};

/// Parses and validates the URL a chart index will be read from.
///
/// Refuses a URL that does not parse, one that carries userinfo, one that
/// carries a query or a fragment, and one [`decide`] would refuse as the
/// first hop of a chain — so a caller can never send the initial request
/// before this has agreed to it.
///
/// # A credential this reader was never handed
///
/// `https://user:pass@host/index.yaml` parses, and `reqwest` would turn that
/// userinfo into a `Basic` auth header on every request sent to it — which
/// contradicts the one guarantee `HelmCharts` makes: that it carries no
/// credential at all. Refusing it here means a credential slipped into a
/// repository address is never silently honoured. The refusal itself names
/// the address through [`shown`], not the parsed [`Url`](reqwest::Url)'s own
/// `Display`, so the credential this catches never turns up a second time in
/// the very message reporting it.
///
/// # The suffix this reader appends has to land in the path
///
/// The index URL is built by appending `/index.yaml` to a configured
/// repository address with plain string concatenation, not URL-aware
/// joining. A repository address ending in a query (`?x=y`) or a fragment
/// (`#x`) would absorb that suffix into the query or fragment instead of the
/// path — `https://host/repo#x/index.yaml` requests `https://host/repo`, the
/// repository's root, never `/index.yaml` at all. Refusing any query or
/// fragment on the URL this reader is about to request catches that before a
/// request goes out for the wrong document.
///
/// # Errors
///
/// [`RegistryError::Refused`] naming the URL and which rule it failed.
pub(in crate::charts) fn validated_index_url(
    transport: Transport,
    raw: &str,
) -> Result<reqwest::Url, RegistryError> {
    // `raw` is never interpolated into this message, even though
    // `url::ParseError`'s `Display` is a fixed string per failure kind and
    // never echoes the text it failed on -- `raw` is whatever a caller
    // configured as a repository address, which might itself carry a
    // credential pasted straight into it, and this reader does not get to
    // assume that risk away just because today's parser happens not to.
    let url = reqwest::Url::parse(raw).map_err(|error| RegistryError::Refused {
        detail: format!("the chart index address does not parse: {error}"),
    })?;

    if !url.username().is_empty() || url.password().is_some() {
        return Err(RegistryError::Refused {
            detail: format!(
                "the chart index address at {} carries a credential in the URL itself",
                shown(&url)
            ),
        });
    }

    if url.query().is_some() || url.fragment().is_some() {
        return Err(RegistryError::Refused {
            detail: format!(
                "the chart index address at {} carries a query or a fragment",
                shown(&url)
            ),
        });
    }

    decide(transport, &[], &url).map_err(|detail| RegistryError::Refused { detail })?;

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_url_carrying_userinfo_is_refused() {
        assert!(validated_index_url(Transport::Https, "https://user:pass@example.test/index.yaml").is_err());
        assert!(validated_index_url(Transport::Https, "https://user@example.test/index.yaml").is_err());
    }

    #[test]
    fn an_index_url_carrying_a_query_or_fragment_is_refused() {
        // `{repository}/index.yaml` is built by string concatenation, so a
        // trailing query or fragment on the repository address would absorb
        // that suffix instead of the path ever gaining it.
        assert!(validated_index_url(Transport::Https, "https://example.test/repo?x=y/index.yaml").is_err());
        assert!(validated_index_url(Transport::Https, "https://example.test/repo#x/index.yaml").is_err());
    }
}
