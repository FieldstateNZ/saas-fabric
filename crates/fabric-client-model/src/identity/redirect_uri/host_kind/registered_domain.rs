//! The positive rule a claimed-HTTPS host has to satisfy: it is a registered
//! domain.
//!
//! Its own file because it replaced a negative one, and the difference is the
//! whole argument. "Is this an IP address literal?" admits everything the
//! parser being asked has not heard of, and a browser has heard of more
//! spellings than any parser does — `0x` with an empty hexadecimal tail, the
//! fullwidth digits UTS-46 maps back to ASCII, a bracketed authority holding
//! something that is not an address at all. "Is this a registered domain?"
//! admits only what the entitlement is actually about: a name an iOS Universal
//! Link or an Android App Link can be claimed against.
//!
//! Every refusal here says "a registered domain", because that is the one
//! thing the author has to end up with; what follows the comma is which part
//! of it they missed. [`label`] carries the part of the rule that is about one
//! label rather than the whole name.

mod label;

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// The inclusive maximum length of a domain name (RFC 1035 §2.3.4).
const MAX_LENGTH: usize = 253;

/// What an author is told when the host is not ASCII.
const A_LABEL: &str = "a registered domain in A-label form — an internationalised callback is \
                       written as its xn-- encoding, because that is the name the browser \
                       resolves and the name an App Link is claimed against";

/// What an author is told when the host is an address literal rather than a
/// name.
///
/// A colon reaches here from one place only: a bracketed authority
/// `reject_brackets` found well-formed, such as `[2001:db8::1]`. Refused
/// before the label rules, because "at least two labels" is true of a domain
/// and says nothing at all about an address.
const NOT_AN_ADDRESS: &str = "a registered domain, not an IP address";

/// What an author is told when the whole name is too long.
const TOO_LONG: &str = "a registered domain of at most 253 characters";

/// What an author is told when there is only one label.
const ONE_LABEL: &str = "a registered domain of at least two labels — a single-label name is \
                         whatever this network's resolver decides it is, which is not something \
                         an entitlement can be stated against";

/// What an author is told when the name ends in something a browser reads as a
/// number.
const ENDS_IN_A_NUMBER: &str = "a registered domain, not an address — a final label that is all \
                                digits, or that begins 0x, is what makes a host an IPv4 candidate \
                                rather than a name to resolve";

/// Refuses a host that is not a registered domain.
///
/// Reached only for `https` on a host that is neither loopback nor
/// `.internal`, so what is left is the production rule: a name in the public
/// DNS that somebody registered and can therefore prove they control.
///
/// An **empty** label is not checked here. [`super::classify`] refuses one
/// first, for every scheme rather than only this arm, and two messages for one
/// condition would leave a reader wondering which rule they broke.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] naming the part of the rule the
/// host missed, or [`IdentifierError::DisallowedCharacter`] naming the first
/// character that is not a hostname character.
pub(super) fn check(host: &str) -> Result<(), IdentifierError> {
    if !host.is_ascii() {
        return Err(unadmitted(A_LABEL));
    }

    if host.contains(':') {
        return Err(unadmitted(NOT_AN_ADDRESS));
    }

    if host.len() > MAX_LENGTH {
        return Err(unadmitted(TOO_LONG));
    }

    if host.split('.').count() < 2 {
        return Err(unadmitted(ONE_LABEL));
    }

    for part in host.split('.') {
        label::check(part)?;
    }

    if host.rsplit('.').next().is_some_and(ends_in_a_number) {
        return Err(unadmitted(ENDS_IN_A_NUMBER));
    }

    Ok(())
}

/// The URL Standard's "ends in a number" test, tightened at one point.
///
/// A host whose final label is all digits, or begins `0x`, is not resolved as
/// a name at all — it is parsed as an IPv4 address, which is how `0x` on its
/// own reaches `0.0.0.0` and therefore the machine the browser is already on.
/// Only the lower-cased `0x` is tested, for the reason above.
///
/// **Stricter than the standard**, deliberately. The standard counts a label
/// as a number only when the tail after `0x` is hexadecimal; this refuses any
/// `0x` prefix. It narrows a refusal rather than widening one, and all it
/// costs is a final label beginning `0x` with a non-hex character in it — a
/// name no callback has needed, and one an author keeps by putting it
/// anywhere but last.
fn ends_in_a_number(label: &str) -> bool {
    label.starts_with("0x") || (!label.is_empty() && label.chars().all(|c| c.is_ascii_digit()))
}

/// The refusal every rule in this file produces, bar the character one.
fn unadmitted(expected: &'static str) -> IdentifierError {
    IdentifierError::Unadmitted { kind: KIND, expected }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_registered_domain_is_admitted() {
        assert!(check("www.example.com").is_ok());
        assert!(check("a-b.example.co.nz").is_ok());
        assert!(check("xn--80ak6aa92e.com").is_ok());
    }

    #[test]
    fn an_internationalised_host_is_refused_in_favour_of_its_a_label() {
        // The fullwidth spelling of `127.0.0.1`. UTS-46 maps it back to the
        // ASCII digits, so a browser resolves the loopback address from it.
        let error = check("１２７．０．０．１").unwrap_err();

        assert!(error.to_string().contains("xn--"), "{error}");
    }

    #[test]
    fn an_address_literal_is_told_it_is_an_address_and_not_that_it_wants_a_second_label() {
        // Reaches here from a bracketed authority `reject_brackets` admits:
        // `[2001:db8::1]` and `[::]` are well-formed IPv6 literals, and the
        // host they yield carries the colons. "At least two labels" — what
        // the label rules used to say about them — is true of a domain and
        // says nothing about an address.
        for host in ["2001:db8::1", "::"] {
            let error = check(host).unwrap_err();

            assert!(error.to_string().contains("not an IP address"), "{host}: {error}");
        }
    }

    #[test]
    fn a_registered_domain_has_at_least_two_labels() {
        assert!(check("intranet").is_err());
        assert!(check("0x").is_err());
    }

    #[test]
    fn an_underscore_is_not_a_hostname_character() {
        assert!(check("my_host.example.com").is_err());
        assert!(check("[foo].example.com").is_err());
        assert!(check("a%2eb.example.com").is_err());
    }

    #[test]
    fn a_label_may_not_hang_a_hyphen_off_either_end() {
        assert!(check("-example.com").is_err());
        assert!(check("example-.com").is_err());
        assert!(check("a-b.example.com").is_ok());
    }

    #[test]
    fn a_label_is_at_most_sixty_three_characters() {
        assert!(check(&format!("{}.com", "a".repeat(63))).is_ok());
        assert!(check(&format!("{}.com", "a".repeat(64))).is_err());
    }

    #[test]
    fn a_name_is_at_most_two_hundred_and_fifty_three_characters() {
        let long = std::iter::repeat_n("abcdefghij", 26)
            .collect::<Vec<_>>()
            .join(".");

        assert!(long.len() > MAX_LENGTH);
        assert!(check(&long).is_err());
    }

    #[test]
    fn a_name_ending_in_a_number_is_an_address_a_browser_would_dial() {
        assert!(check("93.184.216.34").is_err());
        assert!(check("example.123").is_err());
        assert!(check("0x.0x.0x.0x").is_err());
    }

    #[test]
    fn a_number_that_is_not_the_final_label_is_only_a_label() {
        assert!(check("123.example.com").is_ok());
        assert!(check("0x7f.example.com").is_ok());
    }
}
