//! Which addresses a registry may be read at, enforced (ADR 0026 section 5).
//!
//! # Two checks, because there are two ways to name an address
//!
//! A name is resolved by the client's own resolver, [`PublicResolver`], on
//! every connection, and only its public addresses are dialled — so a name
//! that resolves somewhere else tomorrow, or to a private address in the
//! same answer as a public one, never reaches anything private. An IP literal
//! is never resolved: the connector dials it as written. So a URL naming one
//! is refused before its request is built — a redirect target, a token
//! realm, a blob's CDN — and a registered registry's endpoint may not be one.
//!
//! A pagination `Link` needs no check of its own: it is followed only on the
//! registry's own origin, whose host is a name.
//!
//! # A refusal reads the same wherever it happens
//!
//! [`NOT_PUBLIC`] is the whole message, whether a literal was refused or a
//! resolved name was, on the first request or the tenth hop: which of them
//! it was is no business of an operator, and the target is never shown.

#[cfg(test)]
mod address_tests;
mod ranges;
mod resolver;

use std::sync::Arc;

use fabric_platform_management::RegistryError;

pub(crate) use resolver::PublicResolver;

use crate::settings::AddressPolicy;
use crate::transport::is_ip_literal;

/// Every refusal of an address, in full.
pub(crate) const NOT_PUBLIC: &str =
    "a registry an operator registered is read only at public addresses, and this address is not one";

/// A registry's address policy, as its client enforces it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Address {
    /// Public addresses only, or any.
    policy: AddressPolicy,

    /// A test switch: loopback counts as public.
    loopback_is_public: bool,
}

impl Address {
    /// The policy, and whether loopback counts as public.
    pub(crate) fn new(policy: AddressPolicy, loopback_is_public: bool) -> Self {
        Self {
            policy,
            loopback_is_public,
        }
    }

    /// Whether `url` names an IP literal this policy never follows.
    pub(crate) fn refuses_literal(self, url: &reqwest::Url) -> bool {
        self.policy == AddressPolicy::PublicOnly && is_ip_literal(url)
    }

    /// Refuses `url` before any request if it names an IP literal this
    /// policy never follows.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Refused`] with [`NOT_PUBLIC`].
    pub(crate) fn check(self, url: &reqwest::Url) -> Result<(), RegistryError> {
        if self.refuses_literal(url) {
            return Err(refused());
        }
        Ok(())
    }

    /// The resolver a client of this policy resolves names through, if any.
    pub(crate) fn resolver(self) -> Option<Arc<PublicResolver>> {
        (self.policy == AddressPolicy::PublicOnly)
            .then(|| Arc::new(PublicResolver::system(self.loopback_is_public)))
    }
}

/// The refusal of an address.
pub(crate) fn refused() -> RegistryError {
    RegistryError::Refused {
        detail: NOT_PUBLIC.to_owned(),
    }
}

/// What [`PublicResolver`] fails with, found again in a send's error chain.
#[derive(Debug)]
pub(crate) struct NotPublic;

impl std::fmt::Display for NotPublic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(NOT_PUBLIC)
    }
}

impl std::error::Error for NotPublic {}

/// Whether `error`, or anything it was caused by, is [`NotPublic`].
///
/// An `io::Error`'s `source()` skips the error it wraps, so a wrapped one is
/// looked inside explicitly.
pub(crate) fn refused_in(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(error);
    while let Some(current) = next {
        let wrapped = current
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .is_some_and(<dyn std::error::Error + Send + Sync>::is::<NotPublic>);
        if current.is::<NotPublic>() || wrapped {
            return true;
        }
        next = current.source();
    }
    false
}
