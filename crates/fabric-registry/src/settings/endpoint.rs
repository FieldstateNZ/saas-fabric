//! Checking where a registry is served before anything is built on it.
//!
//! Every message names the field, never its value: an address pasted with a
//! credential in it must not come back in an error.

use crate::transport::{is_ip_literal, is_loopback};

/// A registered registry's endpoint: an HTTPS origin, named by a host.
///
/// # Why an origin, and why never an IP literal
///
/// Every request is built by appending to it, so a path, a credential, a
/// query or a fragment would ride along on every one. And a registry is
/// named by its host — that is how every image reference names it — so an
/// endpoint that is an address is one no repository could be named under,
/// and one the address policy would refuse on every request anyway.
pub(super) fn origin(text: &str, field: &'static str) -> Result<reqwest::Url, String> {
    let url = parsed(text, field)?;

    if url.path() != "/" {
        return Err(format!("registry: {field} carries a path; it must be an origin"));
    }
    if is_ip_literal(&url) {
        return Err(format!(
            "registry: {field} is an IP address; a registry is named by its host"
        ));
    }
    if url.scheme() != "https" {
        return Err(format!("registry: {field} is not an HTTPS URL"));
    }

    Ok(url)
}

/// The deployment's configured base URL, held to what it always was: no
/// credential, query or fragment. Its scheme is judged when the client is
/// built, by the same rule as every redirect.
pub(super) fn base_url(text: &str, field: &'static str) -> Result<reqwest::Url, String> {
    parsed(text, field)
}

/// A loopback address a test serves a registry from.
pub(super) fn loopback(text: &str, field: &'static str) -> Result<reqwest::Url, String> {
    let url = parsed(text, field)?;
    if !is_loopback(&url) || !matches!(url.scheme(), "http" | "https") {
        return Err(format!(
            "registry: {field} is neither HTTPS nor plain HTTP to a loopback host"
        ));
    }
    Ok(url)
}

/// How a registry served at `url` names its repositories: its host, with the
/// port when it names one other than the scheme's.
pub(super) fn naming_host(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

/// Parsed, with a host, and carrying no credential, query or fragment.
fn parsed(text: &str, field: &'static str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(text).map_err(|_| format!("registry: {field} is not a URL"))?;

    if url.cannot_be_a_base() || url.host_str().is_none_or(str::is_empty) {
        return Err(format!("registry: {field} names no host"));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "registry: {field} carries a credential, a query or a fragment"
        ));
    }

    Ok(url)
}
