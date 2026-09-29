//! Building a registry client: from its settings, or by the deployment's two
//! constructors, and the checks every one makes before anything is sent.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::address::Address;
use crate::client::presented::Presented;
use crate::client::realm::Realm;
use crate::client::verified::Verified;
use crate::client::{http, OciRegistry};
use crate::settings::RegistrySettings;
use crate::transport::{permits, Transport};

impl OciRegistry {
    /// Builds the deployment's registry client, which only ever speaks HTTPS,
    /// e.g. `new("https://ghcr.io", "ghcr.io", 30)`: anonymous, following its
    /// own challenge, on any network — [`RegistrySettings::deployment`].
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
        Self::with_settings(
            RegistrySettings::deployment(&base_url.into(), registry_host)?,
            timeout_seconds,
        )
    }

    /// Builds a client for a test serving a registry from a loopback socket,
    /// which has no certificate to offer.
    ///
    /// The only way the deployment's client accepts plain HTTP, and only to
    /// a loopback host; a redirect off loopback on plain HTTP, or back to
    /// HTTP after an HTTPS hop, is still refused. `#[doc(hidden)]` because
    /// nothing in production calls it: it is this crate's test suite's alone.
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
        let base_url = base_url.into();
        Self::with_settings(
            RegistrySettings::deployment(&base_url, registry_host)?.serve_from(&base_url)?,
            timeout_seconds,
        )
    }

    /// Builds a client for one registry, as its settings describe it.
    ///
    /// # Errors
    ///
    /// Returns a message, naming the field and never its value, if the
    /// endpoint is not HTTPS, is an IP address a registry held to public
    /// addresses may not be, if a realm is not a URL, if the timeout is
    /// zero, or if an HTTP client cannot be built.
    pub fn with_settings(settings: RegistrySettings, timeout_seconds: u64) -> Result<Self, String> {
        let field = settings.field;
        let origin =
            reqwest::Url::parse(&settings.endpoint).map_err(|_| format!("registry: {field} is not a URL"))?;
        let transport = settings.transport;

        if permits(transport, &[], &origin).is_err() {
            return Err(match transport {
                Transport::Https => format!("registry: {field} is not an HTTPS URL"),
                Transport::LoopbackToo => {
                    format!("registry: {field} is neither HTTPS nor plain HTTP to a loopback host")
                }
            });
        }

        // Only ever set through `serve_from`, and honoured only there: a
        // client that speaks HTTPS never counts loopback as public.
        let loopback_is_public = settings.loopback_is_public && transport == Transport::LoopbackToo;
        let address = Address::new(settings.address, loopback_is_public);
        if address.refuses_literal(&origin) {
            return Err(format!(
                "registry: {field} is an IP address, and this registry is read only at public addresses by name"
            ));
        }

        if timeout_seconds == 0 {
            // reqwest reads zero as "no timeout", which is the difference
            // between a bounded discovery pass and one that hangs.
            return Err("registry: timeout_seconds is zero".to_owned());
        }

        let (api, blobs) = http::clients(Duration::from_secs(timeout_seconds), transport, address)?;

        Ok(Self {
            api,
            blobs,
            transport,
            address,
            base_url: origin.as_str().trim_end_matches('/').to_owned(),
            origin,
            realm: Realm::from_rule(&settings.realm)?,
            honours_basic: settings.honours_basic,
            credential: settings
                .credential
                .map(|credential| Presented::new(credential, &settings.naming_host)),
            registry_host: settings.naming_host,
            tokens: Mutex::new(BTreeMap::new()),
            verified: Verified::default(),
        })
    }
}
