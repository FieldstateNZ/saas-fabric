//! Splitting a redirect URI into the parts the rules are stated about.
//!
//! Split from the newtype because getting the authority out of a URI is where
//! a substring test would quietly do the wrong thing, and that deserves to be
//! read on its own rather than found among length checks. The decision about
//! *which* schemes and hosts are permitted lives in [`super::kind`], which is
//! the only copy of it; this module hands that decision the right substrings.

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// What this model expects in place of userinfo in the authority.
const EXPECTED_NO_USERINFO: &str =
    "no userinfo in the authority — a redirect URI has no legitimate use for credentials";

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

/// The byte index of the `*` that spells "any port", if the URI has one.
///
/// A wildcard is otherwise permitted only in the final position, so the parser
/// needs to know exactly which `*` this is rather than whether one exists —
/// `https://example.com/a:*/b` has a `*` after a colon and it is in the path,
/// which is the mistake a looser test would make. Whether a strategy may
/// *hold* a wildcard port is a separate question, answered in
/// `redirect_strategy::rules`.
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
