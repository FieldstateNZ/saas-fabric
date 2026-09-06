//! Level two of the partition: which kind a *host* makes a URI, once the
//! scheme has admitted it.
//!
//! Its own file because the loopback boundary is the subtlest security
//! argument in this crate, and it is the one a reader most needs to meet on
//! its own rather than at the end of a scheme table. [`ip_literal`] carries
//! the numeric parsing this file's rules are stated against, so the decision
//! tree here stays readable as a decision tree.
//!
//! Over the 120-line advisory threshold. The reason is that this is one
//! decision tree — `classify` — together with the two trivial predicates it
//! calls and the constants naming what each refusal tells the author; none of
//! those would be reused or tested apart from the tree that calls them.

mod ip_literal;

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

/// What an author is told when the host is empty, or a label between two dots
/// is.
const NO_EMPTY_LABEL: &str = "a host, not an empty string or an empty label between two dots";

/// What an author is told when a claimed-HTTPS host is an IP address literal.
const NOT_A_DOMAIN: &str = "a registered domain, not an IP address";

/// Classifies an already-lower-cased host.
///
/// Empty first, then loopback, then `.internal`, then everything else — and
/// everything else only if the scheme was `https`, and then only if it is not
/// itself an IP address literal. `https://localhost:5173/cb` is therefore
/// **loopback**, not the production kind, and `https://admin.corp.internal/cb`
/// is a private-network host: a scheme-only partition would put both in the
/// production kind and let a production strategy hold a development callback
/// while looking correct.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] for an empty host or label, for a
/// host that reaches loopback without being one of [`LOOPBACK`]'s spellings,
/// and for an IP address literal offered under `https`; and
/// [`IdentifierError::BadBoundary`] for plain HTTP on a public host.
pub(super) fn classify(host: &str, secure: bool) -> Result<RedirectUriKind, IdentifierError> {
    if host.split('.').any(str::is_empty) {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: NO_EMPTY_LABEL,
        });
    }

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

    if !secure {
        return Err(IdentifierError::BadBoundary { kind: KIND });
    }

    if is_ip_literal(host) {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: NOT_A_DOMAIN,
        });
    }

    Ok(RedirectUriKind::Https)
}

/// Whether a host reaches the loopback interface without being one of the
/// three spellings this model recognises.
///
/// Refused rather than admitted or classified as an ordinary public host.
/// `127.0.0.2`, the abbreviated `127.1`, the all-numeric `2130706433`, the
/// hexadecimal `0x7f000001` and the IPv4-mapped `[::ffff:127.0.0.1]` all reach
/// loopback for `inet_aton` — and therefore for curl, a browser, and most
/// libc resolvers — without being one of the three spellings this model
/// recognises; `localhost.localdomain` reaches it by resolving on many
/// machines instead. An entitlement satisfied by an address that never leaves
/// the machine, or that can only be recognised by resolving a name, is the
/// entitlement failing to mean anything.
fn reaches_loopback(host: &str) -> bool {
    if let Some(address) = ip_literal::parse(host) {
        return ip_literal::is_loopback(address);
    }

    host.starts_with("localhost.")
}

/// Whether a host is an IP address literal, in any spelling — not only the
/// ones that reach loopback.
///
/// Reached only once loopback and `.internal` are ruled out: an ordinary
/// public address is not a registered domain, and a Universal Link or App
/// Link needs one. Plain HTTP to the same address is refused already, for the
/// unrelated reason that plain HTTP is refused on any public host.
fn is_ip_literal(host: &str) -> bool {
    ip_literal::parse(host).is_some()
}
