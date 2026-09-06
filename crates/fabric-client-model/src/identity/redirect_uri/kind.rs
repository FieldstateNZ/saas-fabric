//! Which of four kinds a redirect URI is, and level one of the rule that
//! decides: the scheme.
//!
//! The one place classification lives. `authority` hands it substrings and
//! `host_kind` answers level two; nothing keeps a second copy, because two
//! copies of a security partition are two answers waiting to disagree.

use std::fmt;

use fabric_core::IdentifierError;

use super::{authority, host_kind, private_use_scheme};

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// What this model expects in place of a scheme it does not classify.
const EXPECTED_SCHEME: &str = "https, http, or a private-use scheme in reverse-domain form";

/// What kind of callback a redirect URI is.
///
/// The partition a redirect *strategy* is stated against: a client declares
/// which kind it is entitled to, and a URI outside that kind is refused rather
/// than quietly accepted because it happens to parse. Without it a production
/// client and a development client are indistinguishable in the document, and
/// either may hold the other's callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RedirectUriKind {
    /// A private-use URI scheme a native application registered with the
    /// operating system, whatever its authority.
    PrivateUseScheme,

    /// The machine the browser is already on.
    Loopback,

    /// A host under the ICANN-reserved `.internal` top-level domain.
    PrivateNetwork,

    /// A public host over TLS. The production rule, and what an iOS Universal
    /// Link or an Android App Link is.
    Https,
}

impl fmt::Display for RedirectUriKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PrivateUseScheme => "a private-use scheme callback",
            Self::Loopback => "a loopback callback",
            Self::PrivateNetwork => "a private-network callback",
            Self::Https => "a public https callback",
        })
    }
}

/// Classifies a redirect URI: scheme first, then host, both lower-cased.
///
/// # Why the scheme decides first
///
/// `nz.fieldstate.slipway://localhost/cb` is a private-use scheme, not a
/// loopback callback. A host-first partition would classify it by the
/// `localhost` in its authority and hand a native application's callback the
/// entitlement a development HTTP callback has — two different security
/// arguments collapsed by a string that happens to appear in both. The
/// authority of a private-use URI is not a network location at all; it is
/// whatever the application put there, and RFC 8252 §7.1's own examples put
/// nothing there.
///
/// # Why plain HTTP reaches level two at all
///
/// A redirect URI is where an authorisation code is delivered, and over plain
/// HTTP that code is readable by anything on the path. So `https://` is the
/// rule, and the exceptions are the two cases where requiring TLS would
/// require a certificate that **cannot exist**: loopback, where the code never
/// leaves the machine, and the private top-level domain no public certificate
/// authority will issue for. Both are [`host_kind`]'s to decide.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`], naming the schemes this model
/// classifies, for a scheme it does not — `javascript:`, `data:` and `file:`
/// among them, and a `www.example.com:8080/cb` missing its scheme entirely —
/// or an authority carrying userinfo, and whatever [`host_kind::classify`]
/// refuses.
pub(super) fn classify(value: &str) -> Result<RedirectUriKind, IdentifierError> {
    let unadmitted = || IdentifierError::Unadmitted {
        kind: KIND,
        expected: EXPECTED_SCHEME,
    };

    let (scheme, rest) = value.split_once(':').ok_or_else(unadmitted)?;
    let scheme = scheme.to_ascii_lowercase();

    if private_use_scheme::is_private_use(&scheme, rest) {
        return Ok(RedirectUriKind::PrivateUseScheme);
    }

    let secure = scheme == "https";
    if !secure && scheme != "http" {
        return Err(unadmitted());
    }

    let authority = authority::of(rest.strip_prefix("//").ok_or_else(unadmitted)?);
    authority::reject_userinfo(authority)?;

    host_kind::classify(&authority::host(authority).to_ascii_lowercase(), secure)
}
