//! Which schemes and hosts a redirect URI may name.
//!
//! Split from the newtype because this is the security rule, and it deserves
//! to be read on its own rather than found among length checks.

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// Hosts that are the machine the browser is already on.
const LOOPBACK: [&str; 2] = ["localhost", "127.0.0.1"];

/// The top-level domain ICANN reserved for private-use applications.
///
/// Its board resolved in July 2024 to withhold `.internal` from delegation
/// permanently, for exactly this purpose: names that resolve only inside an
/// organisation. It is what makes the plain-HTTP exception below a rule rather
/// than a favour to one deployment.
const PRIVATE_TLD: &str = "internal";

/// Checks the scheme and host of a redirect URI.
///
/// # Why plain HTTP is permitted at all
///
/// A redirect URI is where an authorisation code is delivered, and over plain
/// HTTP that code is readable by anything on the path. So `https://` is the
/// rule, and the exceptions are the two cases where requiring TLS would
/// require a certificate that **cannot exist**:
///
/// - **Loopback.** The code never leaves the machine. This is what RFC 8252
///   recommends for native applications, for the same reason.
/// - **The `.internal` top-level domain.** ICANN resolved in July 2024 to
///   withhold it from delegation permanently, reserving it for private-use
///   applications. Because it will never exist in the public DNS root, it
///   cannot resolve on the internet and no public certificate authority will
///   issue for it — so an internal environment reached over plain HTTP is not
///   a deployment that *should* have TLS and skipped it; it is one where the
///   public TLS ecosystem does not apply.
///
/// Everything else must be `https://`. `http://www.example.com` is refused, and
/// that is the case this rule exists for.
///
/// # What the host check must not be
///
/// A substring test. `.internal` appearing *anywhere* in the URI is not the
/// question — `http://evil.example.com/.internal` contains it and is a public
/// host. Only the authority is examined, with any port and any path, query or
/// fragment removed first.
///
/// Userinfo is refused outright rather than parsed around. `http://x.internal@
/// evil.example.com/` is a public host wearing an internal-looking prefix, and
/// a redirect URI has no legitimate use for credentials in it.
///
/// # Errors
///
/// Returns [`IdentifierError::BadBoundary`] if the scheme is not permitted, if
/// the authority carries userinfo, or if a plain-HTTP URI names a host that is
/// neither loopback nor `.internal`.
pub(super) fn check(value: &str) -> Result<(), IdentifierError> {
    let refused = || IdentifierError::BadBoundary { kind: KIND };

    if let Some(rest) = value.strip_prefix("https://") {
        return reject_userinfo(authority(rest));
    }

    let rest = value.strip_prefix("http://").ok_or_else(refused)?;
    let authority = authority(rest);
    reject_userinfo(authority)?;

    if is_permitted_over_plain_http(host(authority)) {
        Ok(())
    } else {
        Err(refused())
    }
}

/// The authority: everything before the path, query, or fragment.
fn authority(rest: &str) -> &str {
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// The host: the authority with any port removed.
///
/// Splitting on the *last* colon would be wrong for a bracketed IPv6 literal,
/// and splitting on the first is wrong for the same reason. Neither matters
/// here: an IPv6 literal is not loopback-by-name and not `.internal`, so it is
/// refused over plain HTTP whichever way it is cut.
fn host(authority: &str) -> &str {
    authority.split(':').next().unwrap_or(authority)
}

/// Refuses an authority carrying userinfo.
fn reject_userinfo(authority: &str) -> Result<(), IdentifierError> {
    if authority.contains('@') {
        return Err(IdentifierError::BadBoundary { kind: KIND });
    }

    Ok(())
}

/// Whether a host may be reached over plain HTTP.
fn is_permitted_over_plain_http(host: &str) -> bool {
    let loopback = LOOPBACK.contains(&host);
    let private = host == PRIVATE_TLD || host.ends_with(&format!(".{PRIVATE_TLD}"));

    loopback || private
}
