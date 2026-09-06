//! What one label of a registered domain may be.
//!
//! Split from the whole-name rule because the two answer different questions.
//! [`super::check`] asks whether a *name* is one somebody could register —
//! its script, its length, how many labels it has, and whether a browser
//! would dial it as an address instead of resolving it. This file asks the
//! only question left, of each label in turn: is this a hostname label at
//! all. Every host that reaches here has already passed the first question,
//! so the two never have to be read together.

use fabric_core::IdentifierError;

/// The inclusive maximum length of one label (RFC 1035 §2.3.4).
const MAX_LABEL: usize = 63;

/// What an author is told when a label carries something that is not a
/// hostname character. Naming the four that actually get written, because
/// `[`, `]` and `%` arrive from a bracketed authority and `_` from a service
/// record somebody assumed was a hostname.
const HOST_CHARACTERS: &str = "a registered domain, whose labels are letters, digits and hyphens \
                               — not brackets, underscores or percent signs";

/// What an author is told when one label is too long.
const LABEL_TOO_LONG: &str = "a registered domain whose labels are at most 63 characters";

/// What an author is told when a label begins or ends with a hyphen.
const HYPHEN_BOUNDARY: &str = "a registered domain whose labels neither start nor end with a hyphen";

/// Refuses one label that is too long, carries a character a hostname may not,
/// or hangs a hyphen off either end.
///
/// An **empty** label is not checked here. `super::super::classify` refuses
/// one first, for every scheme rather than only the production arm, and two
/// messages for one condition would leave a reader wondering which rule they
/// broke.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] naming the part of the rule the
/// label missed, or [`IdentifierError::DisallowedCharacter`] naming the first
/// character that is not a hostname character.
pub(super) fn check(label: &str) -> Result<(), IdentifierError> {
    if label.len() > MAX_LABEL {
        return Err(super::unadmitted(LABEL_TOO_LONG));
    }

    if let Some(character) = label.chars().find(|character| !is_host_character(*character)) {
        return Err(IdentifierError::DisallowedCharacter {
            kind: super::KIND,
            character,
            expected: HOST_CHARACTERS,
        });
    }

    if label.starts_with('-') || label.ends_with('-') {
        return Err(super::unadmitted(HYPHEN_BOUNDARY));
    }

    Ok(())
}

/// Whether a character may appear in a hostname label.
///
/// Lower case only, because `super::super::super::kind::classify` lower-cases
/// the host before this file ever sees it.
fn is_host_character(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_label_is_admitted() {
        assert!(check("a-b").is_ok());
        assert!(check("xn--80ak6aa92e").is_ok());
        assert!(check("123").is_ok());
    }

    #[test]
    fn an_underscore_is_not_a_hostname_character() {
        // Legal in a DNS record, never in a hostname. `[`, `]` and `%` arrive
        // from a bracketed authority instead.
        for label in ["my_host", "[foo]", "a%2eb"] {
            assert!(check(label).is_err(), "{label}");
        }
    }

    #[test]
    fn a_label_may_not_hang_a_hyphen_off_either_end() {
        assert!(check("-example").is_err());
        assert!(check("example-").is_err());
    }

    #[test]
    fn a_label_is_at_most_sixty_three_characters() {
        assert!(check(&"a".repeat(MAX_LABEL)).is_ok());
        assert!(check(&"a".repeat(MAX_LABEL + 1)).is_err());
    }
}
