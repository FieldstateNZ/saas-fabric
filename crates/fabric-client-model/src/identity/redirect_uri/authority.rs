//! Splitting a redirect URI into the parts the rules are stated about.
//!
//! Split from the newtype because getting the authority out of a URI is where
//! a substring test would quietly do the wrong thing, and that deserves to be
//! read on its own rather than found among length checks. The decision about
//! *which* schemes and hosts are permitted lives in [`super::kind`], which is
//! the only copy of it; this module hands that decision the right substrings.
//!
//! Over the 120-line advisory threshold. The reason is that splitting a URI
//! and refusing a URI that cannot be split honestly are the same concept read
//! from two sides: `reject_brackets` exists precisely because `host_and_port`
//! would otherwise return a plausible host for `[::1`, and the argument for
//! one is unreadable without the other in front of it. What is here is six
//! short functions over one string, none of which would be reused or tested
//! apart from the rest.

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// What this model expects in place of userinfo in the authority.
const EXPECTED_NO_USERINFO: &str =
    "no userinfo in the authority — a redirect URI has no legitimate use for credentials";

/// What this model expects of a bracketed authority.
const EXPECTED_BRACKETS: &str = "a bracketed authority closed with ], holding an IPv6 address \
                                 literal and no zone id, followed by a port or nothing";

/// The authority: everything before the path, query, or fragment.
///
/// # What this must not be
///
/// A substring test. `.internal` appearing *anywhere* in the URI is not the
/// question — `http://evil.example.com/.internal` contains it and is a public
/// host. Only the authority is examined, with any path, query or fragment
/// removed first, and the port removed after that by [`host`].
pub(super) fn of(rest: &str) -> &str {
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// The host: the authority with any port removed, and any IPv6 brackets with
/// it.
///
/// Bracket-aware, because it has to be. A bracketed IPv6 literal carries
/// colons of its own, so splitting `[::1]:5173` on the first colon yields `[`
/// and splitting it on the last yields `[::1]`, and neither is the host. This
/// used to be argued away — an IPv6 literal was never loopback-by-name and so
/// was refused over plain HTTP whichever way it was cut — and that argument
/// stopped being true when `::1` joined the loopback set. The rule replaces
/// it: inside brackets the host runs to the closing bracket, and a port, if
/// there is one, follows it.
pub(super) fn host(authority: &str) -> &str {
    host_and_port(authority).0
}

/// The host and the port, split apart with brackets accounted for.
pub(super) fn host_and_port(authority: &str) -> (&str, Option<&str>) {
    if let Some(inside) = authority.strip_prefix('[') {
        return match inside.split_once(']') {
            Some((host, after)) => (host, after.strip_prefix(':')),
            None => (inside, None),
        };
    }

    match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    }
}

/// The byte index of a `*` standing where a port belongs, if the URI has one.
///
/// Found so it can be **refused**, not admitted: a `*` in the port position is
/// a spelling Keycloak matches nothing against (observed on 26.0.8), and
/// `characters::check` needs to tell it apart from the trailing wildcard it
/// does permit. `https://example.com/a:*/b` has a `*` after a colon and it is
/// in the path, which is the mistake a looser test would make.
pub(super) fn wildcard_port_index(value: &str) -> Option<usize> {
    let (scheme, rest) = value.split_once("://")?;
    let authority = of(rest);

    if host_and_port(authority).1? != "*" {
        return None;
    }

    // The `*` is the last byte of the authority, which begins three bytes
    // (`://`) after the scheme.
    scheme.len().checked_add(2)?.checked_add(authority.len())
}

/// Refuses an authority carrying userinfo.
///
/// Refused outright rather than parsed around. `http://x.internal@evil.example.com/`
/// is a public host wearing an internal-looking prefix, and a redirect URI has
/// no legitimate use for credentials in it.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] if the authority contains `@`.
pub(super) fn reject_userinfo(authority: &str) -> Result<(), IdentifierError> {
    if authority.contains('@') {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: EXPECTED_NO_USERINFO,
        });
    }

    Ok(())
}

/// Refuses a bracketed authority this model cannot read as one.
///
/// RFC 3986 puts brackets around exactly one thing: an IP literal. Three
/// spellings get past a split that only looks for the closing bracket, and all
/// three matter because what is inside decides the kind.
///
/// - `[::1` never closes, so [`host_and_port`] hands back `::1` and the URI
///   classifies as **loopback** on the strength of a bracket whose other half
///   nobody wrote.
/// - `[::1%25lo0]` carries a zone id — an interface name, meaningful only on
///   the machine holding it. `::1%25lo0` is not `::1`, and a callback naming a
///   scope no other machine shares is not a declaration.
/// - `[foo.example.com]` holds no colon, so it is not an IPv6 address at all.
///   Left alone it would reach the registered-domain rule and pass it, which
///   is a bracketed authority being read as a domain.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] naming what a bracketed authority
/// is.
pub(super) fn reject_brackets(authority: &str) -> Result<(), IdentifierError> {
    let Some(inside) = authority.strip_prefix('[') else {
        return Ok(());
    };

    let well_formed = inside.split_once(']').is_some_and(|(literal, after)| {
        literal.contains(':') && !literal.contains('%') && (after.is_empty() || after.starts_with(':'))
    });

    if well_formed {
        return Ok(());
    }

    Err(IdentifierError::Unadmitted {
        kind: KIND,
        expected: EXPECTED_BRACKETS,
    })
}
