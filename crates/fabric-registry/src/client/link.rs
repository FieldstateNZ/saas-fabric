//! Following a listing's `Link: <…>; rel="next"` header, on one origin only.

use fabric_platform_management::RegistryError;

use crate::transport::same_origin;

/// The next page `response` names, if it names one.
///
/// A relative target is resolved against the URL that answered. An absolute
/// one is followed only on `origin`, the registry's own: a registry naming
/// another origin would otherwise be telling this adapter where to send its
/// next request, and its pull token with it.
///
/// # Why another origin is `Refused`, not the end of the list
///
/// Stopping there would return the pages read so far as if they were all of
/// them — a truncated listing, which looks exactly like a component whose
/// newer versions do not exist, and discovery would quietly stop advancing.
/// And it is not `Unavailable`: asking again gets the same `Link`, so there
/// is nothing to wait for. The registry answered, and the answer is one this
/// adapter will not follow.
///
/// # Errors
///
/// [`RegistryError::Refused`] naming `operation` for a target on another
/// origin, one carrying a credential, or one that does not parse; never the
/// target itself, which the registry wrote.
pub(super) fn next_page(
    response: &reqwest::Response,
    origin: &reqwest::Url,
    operation: &str,
) -> Result<Option<String>, RegistryError> {
    let refused = |why: &str| RegistryError::Refused {
        detail: format!("{operation} named a next page {why}"),
    };

    let Some(value) = response.headers().get(reqwest::header::LINK) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| refused("in a header that is not text"))?;

    let Some(target) = next_target(value) else {
        return Ok(None);
    };

    let mut next = response
        .url()
        .join(target)
        .map_err(|_| refused("that is not a URL"))?;

    if !same_origin(&next, origin) {
        return Err(refused("on another origin, which is not followed"));
    }
    if !next.username().is_empty() || next.password().is_some() {
        return Err(refused("carrying a credential, which is not followed"));
    }

    next.set_fragment(None);

    Ok(Some(next.into()))
}

/// The target of the `rel="next"` link in a `Link` header value, if any.
///
/// A header may carry several links, comma-separated; only the one whose
/// parameters say `rel="next"` (or `rel=next`) counts.
pub(super) fn next_target(value: &str) -> Option<&str> {
    value.split(',').find_map(|link| {
        let (target, parameters) = link.trim().strip_prefix('<')?.split_once('>')?;
        let is_next = parameters.split(';').any(|parameter| {
            parameter
                .trim()
                .strip_prefix("rel=")
                .is_some_and(|rel| rel.trim_matches('"').split_whitespace().any(|rel| rel == "next"))
        });

        is_next.then_some(target)
    })
}
