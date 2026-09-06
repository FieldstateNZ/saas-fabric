//! Which characters a redirect URI may carry, and where a wildcard may stand.
//!
//! Its own file because the wildcard rule is now two rules that have to agree:
//! the parser decides where a `*` may *appear*, and
//! `redirect_strategy::rules` decides which strategies may *hold* one. The
//! parser widens universally and the strategy narrows, so keeping the
//! "where" here stops the strategy table growing a second copy of itself.

use fabric_core::IdentifierError;

use super::authority;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// The permitted set, described for the error message.
const EXPECTED: &str = "no spaces or control characters, and a wildcard only as the whole port \
                        or as the final character";

/// Refuses whitespace, control characters, and a misplaced wildcard.
///
/// Two positions are permitted and no others. The **final** character, because
/// registering a callback path prefix is the ordinary case. The **whole port**,
/// because RFC 8252 §7.3 requires a loopback redirect to work on whichever
/// ephemeral port a native application binds, and `http://127.0.0.1:*/callback`
/// is what a developer writes when they want to say so.
///
/// Everywhere else is refused, and the case this exists for is a wildcard in
/// the *host*: `https://*.example.com/callback` reads as a subdomain wildcard
/// and would accept a host the operator never intended to trust.
///
/// # Errors
///
/// Returns [`IdentifierError::DisallowedCharacter`] naming the first offending
/// character.
pub(super) fn check(value: &str) -> Result<(), IdentifierError> {
    let last = value.len().saturating_sub(1);
    let wildcard_port = authority::wildcard_port_index(value);

    for (index, character) in value.char_indices() {
        let misplaced_wildcard = character == '*' && index != last && Some(index) != wildcard_port;

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
/// A URI whose only wildcard is its port does not have one: `http://127.0.0.1:*`
/// ends in a `*` and says nothing about the path.
pub(super) fn has_path_wildcard(value: &str) -> bool {
    let last = value.len().saturating_sub(1);

    value.ends_with('*') && authority::wildcard_port_index(value) != Some(last)
}

/// Whether the URI names every port rather than one.
pub(super) fn has_wildcard_port(value: &str) -> bool {
    authority::wildcard_port_index(value).is_some()
}
