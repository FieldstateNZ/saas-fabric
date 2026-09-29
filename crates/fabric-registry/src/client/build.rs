//! Building a registry client: its two constructors, and the checks both
//! make before anything is sent.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::client::verified::Verified;
use crate::client::{http, OciRegistry};
use crate::transport::{permits, Transport};

impl OciRegistry {
    /// Builds a client that only ever speaks HTTPS, e.g.
    /// `new("https://ghcr.io", "ghcr.io", 30)`.
    ///
    /// This is the only constructor the composition root calls. The one
    /// exception, which exists for tests alone, is hidden from this crate's
    /// published docs.
    ///
    /// # Errors
    ///
    /// Returns a message if the registry host is empty, if the base URL is
    /// not an `https://` URL with no credential, query or fragment, if the
    /// timeout is zero, or if an HTTP client cannot be built. The message
    /// names the field and never its value.
    pub fn new(
        base_url: impl Into<String>,
        registry_host: impl Into<String>,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        build(
            &base_url.into(),
            registry_host.into(),
            timeout_seconds,
            Transport::Https,
        )
    }

    /// Builds a client for a test serving a registry from a loopback socket,
    /// which has no certificate to offer.
    ///
    /// The only way an [`OciRegistry`] accepts plain HTTP, and only to a
    /// loopback host; a redirect off loopback on plain HTTP, or back to HTTP
    /// after an HTTPS hop, is still refused. `#[doc(hidden)]` because nothing
    /// in production calls it: it is this crate's test suite's alone.
    ///
    /// # Errors
    ///
    /// The same as [`new`](Self::new), with plain HTTP to loopback allowed.
    #[doc(hidden)]
    pub fn plain_http_to_loopback(
        base_url: impl Into<String>,
        registry_host: impl Into<String>,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        build(
            &base_url.into(),
            registry_host.into(),
            timeout_seconds,
            Transport::LoopbackToo,
        )
    }
}

/// Validates the addresses and builds both clients.
///
/// # Why the base URL is checked here, once
///
/// Every request is built by appending to it, so a credential, query or
/// fragment on it would ride along on every one. And HTTPS begins with it:
/// [`permits`] judges it as the first hop, as it judges every redirect.
fn build(
    base_url: &str,
    registry_host: String,
    timeout_seconds: u64,
    transport: Transport,
) -> Result<OciRegistry, String> {
    if registry_host.trim().is_empty() {
        return Err("registry: registry_host is empty".to_owned());
    }

    let origin = reqwest::Url::parse(base_url).map_err(|_| "registry: base_url is not a URL".to_owned())?;

    if origin.cannot_be_a_base()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err("registry: base_url carries a credential, a query or a fragment".to_owned());
    }

    if permits(transport, &[], &origin).is_err() {
        return Err(match transport {
            Transport::Https => "registry: base_url is not an HTTPS URL".to_owned(),
            Transport::LoopbackToo => {
                "registry: base_url is neither HTTPS nor plain HTTP to a loopback host".to_owned()
            }
        });
    }

    if timeout_seconds == 0 {
        // reqwest reads zero as "no timeout", which is the difference
        // between a bounded discovery pass and one that hangs.
        return Err("registry: timeout_seconds is zero".to_owned());
    }

    let (api, blobs) = http::clients(Duration::from_secs(timeout_seconds), transport)?;

    Ok(OciRegistry {
        api,
        blobs,
        transport,
        base_url: origin.as_str().trim_end_matches('/').to_owned(),
        origin,
        registry_host,
        tokens: Mutex::new(BTreeMap::new()),
        verified: Verified::default(),
    })
}
