//! Level two of the partition: which kind a *host* makes a URI, once the
//! scheme has admitted it.
//!
//! Its own file because the loopback boundary is the subtlest security
//! argument in this crate, and it is the one a reader most needs to meet on
//! its own rather than at the end of a scheme table. [`ip_literal`] carries
//! the numeric parsing the loopback rule is stated against,
//! [`special_use`] carries which top-level domains the public DNS has
//! reserved, and [`registered_domain`] carries the production rule, so the
//! decision tree here stays readable as a decision tree.
//!
//! Over the 120-line advisory threshold. The reason is that this is one
//! decision tree — `classify` — together with the single predicate it calls
//! and the constants naming what each refusal tells the author; none of those
//! would be reused or tested apart from the tree that calls them, and the two
//! rules that would be worth reading alone already are, in the two modules
//! above.

mod ip_literal;
mod registered_domain;
mod special_use;

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

/// What an author is told when plain HTTP reaches a host that is neither
/// loopback nor `.internal`.
///
/// Named as the boundary it is rather than as a parse failure: the value has
/// no typo in it, and telling its author it "must start and end with an
/// alphanumeric character" sends them hunting for one.
const PLAIN_HTTP: &str = "https for a public host; plain http only on loopback or a .internal host";

/// Classifies an already-lower-cased host.
///
/// Empty first, then loopback, then the loopback near-misses, then
/// `.internal`, then plain HTTP is out, then the top-level domains nobody can
/// register — and everything that survives has to prove it is a registered
/// domain. `https://localhost:5173/cb` is therefore
/// **loopback**, not the production kind, and `https://admin.corp.internal/cb`
/// is a private-network host: a scheme-only partition would put both in the
/// production kind and let a production strategy hold a development callback
/// while looking correct.
///
/// # Why the last arm is a positive rule
///
/// It used to ask [`ip_literal::parse`] whether the host was an address and
/// admit it otherwise, which made "everything my parser has not heard of" the
/// production kind. A browser has heard of more spellings than any parser
/// does. [`registered_domain::check`] asks the question the entitlement is
/// actually about instead, and `ip_literal` keeps the one job it is right for:
/// detecting the loopback near-misses, above, for **both** schemes.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] for an empty host or label, for a
/// host that reaches loopback without being one of [`LOOPBACK`]'s spellings,
/// and for plain HTTP on a public host; and whatever
/// [`special_use::check`] and [`registered_domain::check`] refuse.
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
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: PLAIN_HTTP,
        });
    }

    special_use::check(host)?;
    registered_domain::check(host)?;

    Ok(RedirectUriKind::Https)
}

/// Whether a host reaches the loopback interface without being one of the
/// three spellings this model recognises.
///
/// Refused rather than admitted or classified as an ordinary public host.
/// `127.0.0.2`, the abbreviated `127.1`, the all-numeric `2130706433`, the
/// hexadecimal `0x7f000001` and the IPv4-mapped `[::ffff:127.0.0.1]` all reach
/// loopback for `inet_aton` — and therefore for curl, a browser, and most libc
/// resolvers — without being one of the three spellings this model
/// recognises. The two by name reach it differently:
/// `localhost.localdomain` resolves to loopback on many machines, and
/// **every** name under `.localhost` is required to, by RFC 6761 §6.3 —
/// `app.localhost` is loopback in Chrome and Firefox without any resolver
/// being asked. An entitlement satisfied by an address that never leaves the
/// machine, or that can only be recognised by resolving a name, is the
/// entitlement failing to mean anything.
fn reaches_loopback(host: &str) -> bool {
    if let Some(address) = ip_literal::parse(host) {
        return ip_literal::is_loopback(address);
    }

    host.starts_with("localhost.") || special_use::reaches_localhost(host)
}
