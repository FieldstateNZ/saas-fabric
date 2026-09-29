//! Resolving a name to the public addresses it has, and to nothing else.

use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::address::ranges::{is_loopback, is_public};
use crate::address::NotPublic;

/// A lookup: a name to the addresses it has.
type Lookup =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = io::Result<Vec<IpAddr>>> + Send>> + Send + Sync>;

/// The resolver of every client held to public addresses.
///
/// # Why a resolver, and not a check on the URL
///
/// Which address a name reaches is decided when a connection is opened, and
/// only then: a check made earlier on the name would pass a name that
/// resolves to `10.0.0.1` a moment later. `reqwest` asks its resolver for
/// every new connection — to the endpoint, a realm, a CDN — so filtering here
/// holds on every connection, after resolution, whatever the name.
///
/// Only public addresses are handed back; a name with none is refused with
/// [`NotPublic`], which a send's error is searched for.
pub(crate) struct PublicResolver {
    /// How a name is looked up: the system's resolver, or a test's answer.
    lookup: Lookup,

    /// A test switch: loopback counts as public.
    loopback_is_public: bool,
}

impl PublicResolver {
    /// Resolves through the system's resolver, as `tokio` does.
    pub(crate) fn system(loopback_is_public: bool) -> Self {
        Self {
            lookup: Arc::new(|name| {
                Box::pin(async move {
                    let found = tokio::net::lookup_host((name.as_str(), 0)).await?;
                    Ok(found.map(|address| address.ip()).collect())
                })
            }),
            loopback_is_public,
        }
    }

    /// Answers every name with `addresses`, for a test.
    #[cfg(test)]
    pub(crate) fn answering(addresses: Vec<IpAddr>) -> Self {
        Self {
            lookup: Arc::new(move |_| {
                let addresses = addresses.clone();
                Box::pin(async move { Ok(addresses) })
            }),
            loopback_is_public: false,
        }
    }
}

/// Whether `ip` may be dialled.
fn permitted(ip: IpAddr, loopback_is_public: bool) -> bool {
    is_public(ip) || (loopback_is_public && is_loopback(ip))
}

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let lookup = Arc::clone(&self.lookup);
        let loopback_is_public = self.loopback_is_public;
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let public: Vec<SocketAddr> = lookup(name)
                .await?
                .into_iter()
                .filter(|ip| permitted(*ip, loopback_is_public))
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();
            if public.is_empty() {
                return Err(Box::new(NotPublic) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(public.into_iter()) as Addrs)
        })
    }
}
