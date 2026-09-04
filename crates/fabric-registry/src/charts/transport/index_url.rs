//! Validating the address a chart index read is about to start from.

use fabric_platform_management::RegistryError;

use super::{decide, shown, Transport};

/// Parses and validates the URL a chart index will be read from.
///
/// Refuses a URL that does not parse, one that cannot be a base (see below),
/// one that carries userinfo, one that carries a query or a fragment, and
/// one [`decide`] would refuse as the first hop of a chain — so a caller can
/// never send the initial request before this has agreed to it.
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
/// # A cannot-be-a-base address is refused outright
///
/// `oci:user:secret@registry.example/charts` has no `/` right after its
/// scheme, so `url` parses the rest as one opaque blob rather than a host
/// plus userinfo plus path — `username`, `password`, `query`, and `fragment`
/// all report empty, not because there is no credential, but because there
/// is nothing structured enough for the checks above to look inside. This
/// checks [`Url::cannot_be_a_base`](reqwest::Url::cannot_be_a_base) directly
/// and refuses any address shaped that way, whatever it turns out to hold.
///
/// This alone does not make every cannot-be-a-base URL this crate ever sees
/// safe to render, though: `reqwest` runs this crate's own redirect policy
/// (`transport::decide`, wired up as [`policy`](super::policy)) *before* its
/// own scheme checks, so a repository's redirect can still hand [`decide`] a
/// cannot-be-a-base URL that never passes through this function again. What
/// actually makes that safe is [`shown`] itself checking the same thing on
/// every URL it renders — this check exists only so the *first* hop fails
/// with a specific reason instead of falling through to a generic one.
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

    if url.cannot_be_a_base() {
        return Err(RegistryError::Refused {
            detail: format!(
                "the chart index address at {} cannot be read as a hierarchical URL",
                shown(&url)
            ),
        });
    }

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

    #[test]
    fn a_cannot_be_a_base_address_is_refused_even_though_it_carries_no_visible_userinfo() {
        // `username`/`password` report empty for this shape, not because
        // there is no credential, but because there is no host for either to
        // be part of -- so this has to be caught on its own, not folded into
        // the userinfo check above.
        assert!(validated_index_url(Transport::Https, "oci:user:secret@registry.example/charts").is_err());
        assert!(validated_index_url(Transport::Https, "mailto:user:secret@example.test").is_err());
    }
}
