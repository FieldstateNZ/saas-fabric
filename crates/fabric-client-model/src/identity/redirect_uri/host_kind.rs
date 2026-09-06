//! Level two of the partition: which kind a *host* makes a URI, once the
//! scheme has admitted it.
//!
//! Its own file because the loopback boundary is the subtlest security
//! argument in this crate, and it is the one a reader most needs to meet on
//! its own rather than at the end of a scheme table.

use std::net::IpAddr;

use fabric_core::IdentifierError;

use super::kind::RedirectUriKind;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// Hosts that are the machine the browser is already on. These three exactly.
const LOOPBACK: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// The top-level domain ICANN reserved for private-use applications.
///
/// Its board resolved in July 2024 to withhold `.internal` from delegation
/// permanently, for exactly this purpose: names that resolve only inside an
/// organisation. Because it will never exist in the public DNS root it cannot
/// resolve on the internet and no public certificate authority will issue for
/// it — which is what makes the plain-HTTP exception a rule rather than a
/// favour to one deployment.
const PRIVATE_TLD: &str = "internal";

/// What an author is told when a host reaches loopback without being one of
/// the three spellings above.
const BOUNDARY: &str = "loopback is 127.0.0.1, ::1 or localhost, and no other spelling of them";

/// Classifies an already-lower-cased host.
///
/// Loopback first, then `.internal`, then everything else — and everything
/// else only if the scheme was `https`. `https://localhost:5173/cb` is
/// therefore **loopback**, not the production kind, and
/// `https://admin.corp.internal/cb` is a private-network host: a scheme-only
/// partition would put both in the production kind and let a production
/// strategy hold a development callback while looking correct.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] for a host that reaches loopback
/// without being one of [`LOOPBACK`]'s spellings, and
/// [`IdentifierError::BadBoundary`] for plain HTTP on a public host.
pub(super) fn classify(host: &str, secure: bool) -> Result<RedirectUriKind, IdentifierError> {
    if LOOPBACK.contains(&host) {
        return Ok(RedirectUriKind::Loopback);
    }

    if reaches_loopback(host) {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: BOUNDARY,
        });
    }

    if host == PRIVATE_TLD || host.ends_with(&format!(".{PRIVATE_TLD}")) {
        return Ok(RedirectUriKind::PrivateNetwork);
    }

    if secure {
        Ok(RedirectUriKind::Https)
    } else {
        Err(IdentifierError::BadBoundary { kind: KIND })
    }
}

/// Whether a host reaches the loopback interface without being one of the
/// three spellings this model recognises.
///
/// Refused rather than admitted, and refused rather than classified as an
/// ordinary public host. `127.0.0.2` is loopback to every operating system,
/// `[::ffff:127.0.0.1]` is the IPv4-mapped spelling of one that *is* in the
/// list, and `localhost.localdomain` resolves to loopback on many machines. A
/// claimed-HTTPS entitlement satisfied by an address that never leaves the
/// machine is the entitlement failing to mean anything, and an entitlement
/// that can only be recognised by resolving a name is not a declaration.
///
/// A public IPv6 literal is untouched by this: it is parsed, found not to be
/// loopback, and classified like any other host.
fn reaches_loopback(host: &str) -> bool {
    if let Ok(address) = host.parse::<IpAddr>() {
        return match address {
            IpAddr::V4(v4) => v4.is_loopback(),
            IpAddr::V6(v6) => {
                v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|mapped| mapped.is_loopback())
            }
        };
    }

    host.starts_with("localhost.")
}
