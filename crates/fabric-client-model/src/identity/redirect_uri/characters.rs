//! Which characters a redirect URI may carry, and where a wildcard may stand.
//!
//! Its own file because the wildcard rule is two rules that have to agree: the
//! parser decides where a `*` may *appear*, and `redirect_strategy::rules`
//! decides which strategies may *hold* one. The parser widens universally and
//! the strategy narrows, so keeping the "where" here stops the strategy table
//! growing a second copy of itself.

use fabric_core::IdentifierError;

use super::authority;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// The permitted set, described for the error message.
const EXPECTED: &str = "no spaces or control characters, and a wildcard only as the final character";

/// What an author is told when a `*` stands where a port belongs.
///
/// It names the fact rather than the rule, because the author wrote `:*` to
/// mean something and needs to know what to write instead: on Keycloak 26.0.8
/// `http://127.0.0.1:*/cb` matches no redirect at all, while the portless
/// `http://127.0.0.1/cb` matches every port.
const EXPECTED_PORT: &str = "a port, or none — over http a loopback callback registered without a \
                             port already matches any port, and over https, or wherever a port is \
                             written, the match is exact, so name it";

/// Refuses whitespace, control characters, and a misplaced wildcard.
///
/// One position is permitted and no others: the **final** character, because
/// registering a callback path prefix is the ordinary case.
///
/// Everywhere else is refused, and the case this exists for is a wildcard in
/// the *host*: `https://*.example.com/callback` reads as a subdomain wildcard
/// and would accept a host the operator never intended to trust.
///
/// The **port** position gets its own refusal rather than falling into the
/// general one. `http://127.0.0.1:*` puts its `*` last, so the general rule
/// would admit it, and a spelling the identity provider matches nothing
/// against is worse than one it refuses: the client is written, reconciled and
/// converged, and the first login attempt fails somewhere else entirely.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] for a wildcard port, and
/// [`IdentifierError::DisallowedCharacter`] naming the first offending
/// character otherwise.
pub(super) fn check(value: &str) -> Result<(), IdentifierError> {
    if authority::wildcard_port_index(value).is_some() {
        return Err(IdentifierError::Unadmitted {
            kind: KIND,
            expected: EXPECTED_PORT,
        });
    }

    let last = value.len().saturating_sub(1);

    for (index, character) in value.char_indices() {
        let misplaced_wildcard = character == '*' && index != last;

        if character.is_whitespace() || character.is_control() || misplaced_wildcard {
            return Err(IdentifierError::DisallowedCharacter {
                kind: KIND,
                character,
                expected: EXPECTED,
            });
        }
    }

    Ok(())
}

/// Whether the URI ends in a wildcard that stands for a path prefix.
///
/// A trailing `*` is the only one [`check`] admits, so a URI that has one has
/// a path wildcard and nothing else.
pub(super) fn has_path_wildcard(value: &str) -> bool {
    value.ends_with('*')
}
