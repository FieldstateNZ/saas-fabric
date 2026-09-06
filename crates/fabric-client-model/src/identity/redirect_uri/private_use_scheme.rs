//! The private-use URI scheme a native application registers with its
//! operating system.
//!
//! Its own file because this is a distinct security argument: a private-use
//! scheme is not a network location, and no host rule applies to it. The
//! scheme as a *value* — the thing a `customScheme` strategy declares — is in
//! [`app_scheme`]; what is here is the classification.

mod app_scheme;

use fabric_core::IdentifierError;

pub use app_scheme::AppScheme;

/// The label a redirect URI's own refusals carry.
const KIND: &str = "redirect uri";

/// What an author is told when a digit follows the colon.
///
/// It names **both** readings, because the spelling is ambiguous and only the
/// author knows which they meant: `nz.fieldstate.slipway:8080/cb` is a native
/// application's scheme with a port that does not belong to it, and
/// `www.example.com:8080/cb` is a host that lost its `https://`.
const EXPECTED_NO_PORT: &str = "either a private-use callback whose path starts with a slash, as \
                                in nz.fieldstate.slipway:/cb, or a host written with its scheme, \
                                as in https://www.example.com:8080/cb — a digit straight after \
                                the colon reads as a port, which makes what precedes it a host";

/// Whether a URI scheme candidate is a private-use scheme in RFC 8252 §7.1
/// form.
///
/// The RFC asks a native application to use "a domain name under their
/// control, in reverse order". The observable part of that is the dot: an
/// ordinary scheme (`http`, `javascript`, `data`, `file`, `ftp`) has none, so
/// requiring at least one is what keeps this test from swallowing the schemes
/// the classifier exists to refuse.
///
/// It cannot tell a reversed domain from a forward one — `www.example.com` is
/// a syntactically valid scheme — and mostly does not try. What it does check
/// is `rest`, the text after the colon: a digit immediately there means the
/// colon introduced a port, so what precedes it was a host and not a scheme.
/// That case is **refused**, naming both readings, rather than reported as
/// "not a private-use scheme" — which would send the author of
/// `www.example.com:8080/cb` looking for a scheme they never wrote. RFC 8252
/// §7.1's own examples never put a digit straight after the colon:
/// `nz.fieldstate.slipway:/cb` and `nz.fieldstate.slipway://localhost/cb` do
/// not, and stay private-use.
///
/// Nothing is *admitted* by being a private-use scheme: such a URI is only
/// ever accepted under the strategy declaring the same scheme, and that
/// strategy is itself refused until the phase that will carry it.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] when the candidate has a
/// private-use scheme's shape and a digit follows the colon.
pub(super) fn is_private_use(scheme: &str, rest: &str) -> Result<bool, IdentifierError> {
    if AppScheme::check(scheme).is_err() {
        return Ok(false);
    }

    if rest.starts_with(|character: char| character.is_ascii_digit()) {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: EXPECTED_NO_PORT,
        });
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reverse_domain_scheme_is_a_private_use_scheme() {
        assert_eq!(is_private_use("nz.fieldstate.slipway", "/cb"), Ok(true));
        assert_eq!(is_private_use("com.example.app", "://host/cb"), Ok(true));
    }

    #[test]
    fn an_ordinary_scheme_is_not_a_private_use_scheme() {
        // The regression this rule prevents: every one of these would become a
        // private-use scheme under a "not http, therefore private-use" test,
        // and `javascript:` would then be a callback this model accepts.
        for (scheme, rest) in [
            ("http", "//x"),
            ("https", "//x"),
            ("javascript", "alert(1)"),
            ("data", "text/html,x"),
            ("file", "///etc/passwd"),
            ("ftp", "//x"),
        ] {
            assert_eq!(is_private_use(scheme, rest), Ok(false), "{scheme}");
        }
    }

    #[test]
    fn a_digit_after_the_colon_is_a_port_and_the_refusal_names_both_readings() {
        // `www.example.com` is the sharper case: it has a dot and would pass
        // the reverse-domain check on its own. The digit is what says the text
        // before the colon was a host all along — and the author of
        // `nz.fieldstate.slipway:8080/cb` meant the other reading, so the
        // message carries both.
        for scheme in ["www.example.com", "nz.fieldstate.slipway"] {
            let error = is_private_use(scheme, "8080/cb").unwrap_err();

            assert!(error.to_string().contains("nz.fieldstate.slipway:/cb"), "{error}");
            assert!(error.to_string().contains("https://www.example.com"), "{error}");
        }
    }

    #[test]
    fn a_scheme_shaped_candidate_followed_by_a_path_is_still_private_use() {
        // The digit check only fires on a digit. A scheme followed by a path
        // that happens to start with a letter is unaffected.
        assert_eq!(is_private_use("nz.fieldstate.slipway", "alert/cb"), Ok(true));
    }
}
