//! Where a `distribution` registry is served, checked before anything is
//! built on it.
//!
//! Every message names the rule and never the value: an endpoint pasted with
//! a credential in it must not come back in a response or a log.

use crate::registries::{RegistryHost, RegistryKind};

/// A `distribution` registry's endpoint, as an origin, and the host it names
/// its repositories under.
///
/// # Why an origin, and why never an IP literal
///
/// Every request is built by appending to it, so a path, user information, a
/// query or a fragment would ride along on every one. And a registry is named
/// by its host, so an endpoint that is an address is one no repository could
/// be named under — and one a registry held to public addresses would be
/// refused at anyway.
///
/// # Errors
///
/// A message naming the rule the endpoint broke.
pub(super) fn distribution(text: &str) -> Result<(RegistryHost, String), String> {
    if text.contains(['?', '#']) {
        return Err("a registry's endpoint carries no query or fragment".to_owned());
    }
    let uri: http::Uri = text
        .parse()
        .map_err(|_| "a registry's endpoint must be an HTTPS URL".to_owned())?;
    if uri.scheme_str() != Some("https") {
        return Err("a registry's endpoint must be an HTTPS URL".to_owned());
    }
    let Some(authority) = uri.authority() else {
        return Err("a registry's endpoint names no host".to_owned());
    };
    if authority.as_str().contains('@') {
        return Err("a registry's endpoint carries no user information".to_owned());
    }
    if !matches!(uri.path(), "" | "/") {
        return Err("a registry's endpoint is an origin: it carries no path".to_owned());
    }
    if authority.host().starts_with('[') || authority.host().parse::<std::net::Ipv4Addr>().is_ok() {
        return Err("a registry's endpoint is named by its host, never an IP address".to_owned());
    }

    let host = RegistryHost::parse(authority.as_str())?;
    if let Some(kind) = RegistryKind::owning(host.as_str()) {
        return Err(format!(
            "{host} is registered as the {} kind, which fixes its endpoint",
            kind.as_str()
        ));
    }
    let endpoint = format!("https://{host}");
    Ok((host, endpoint))
}

/// Whether two endpoints name one origin, ignoring a trailing `/`.
pub(super) fn same(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}
